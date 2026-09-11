# HarnessHarness — Program LEDGER (product renamed from "MetaHarness" by ADR-0011)

**Single source of truth for program state** (doc 3 §4, §11.8). A re-invoked session reads this file first and resumes at the first non-`done` workstream in the first non-`passed` phase.

- **Program contract:** `docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md` §11 (Operator's Runbook)
- **Terminal deliverables:** `spec/CANONICAL_SPEC.md`, `spec/READINESS_REPORT.md`
- **Hard stop:** Decompose-Readiness Gate (§7.3). No `decompose-spec` / `orchestrate-build` / `implement-spec`; no framework implementation code.
- **Binding constraints:** language/ecosystem decided by **ADR-0050** (2026-09-10; procedure ADR-0009) — contracts stay language-neutral; Phase ≥ 2 artifacts may reference the ADR-0050 ecosystems only for host-ecosystem requirements, protocol SDK availability and sandbox/isolation primitives, by ADR id and layer, never by language name (ADR-0050 §8; audit every pass); `provisional` evidence never load-bearing for C0 (§3.1); every scope change logged as an ADR; **from Phase 0:** the LCD-trap battery (`registers/lcd-test-battery.md`, ADR-0007) is binding on every downstream dossier/ADR; **from Phase 1:** Ontology **v1** (`registers/ontology.md`), the canonical identifiers of ADR-0048, and the settled-for-Phase-2 list in `research/synthesis/phase-1.md` §6 are binding; the settled-for-Phase-1 list (`research/synthesis/phase-0.md`) remains in force.
- **Git:** initialized 2026-09-09; commit at every phase boundary.

## Status legend

`todo` → `in-progress` → `dossier-written` (agent returned, not yet synthesized) → `done` (synthesis pass accepted; ADRs dispositioned) · `blocked` · `deferred(ADR-####)`

## Phase gates

| Phase | Contents | Status | Gate criteria (exit) | Commit |
|---|---|---|---|---|
| Preflight | §11.2 steps 1–6 | **passed** | tree exists; sources/ledger/registers seeded; ADR-0001/0002 ratified | preflight commit |
| 0 | WS-L1 (framing only), WS-A1, WS-L7; Ontology v0 → v0.1; Source Registry seeded + folded | **passed** (2026-09-09) | A1 + L7 dossiers done with ADR-0003/0004/0005 and ADR-0006/0007/0008 ratified; L1 criteria framed, not decided (ADR-0009 proposed); Ontology v0.1 in `registers/ontology.md`; Source Registry folded (S-132…S-166); CF-009 resolved; language-leak audit clean (CF-025) | pending (phase-boundary commit) |
| 1 | WS-A2 A3 A4 A5 · B1 B2 · L2 L3 L4 · I1 I2 | **passed** (2026-09-10) | all 11 done with 36 ratified ADRs (ADR-0012…0047; 27 amended at synthesis) + 2 synthesis ADRs (ADR-0048/0049); Ontology **v1**; one authority/label scheme (ADR-0033/34/35) consistent with HIR/1, the ledger and the trace model; one identity model (ADR-0027/0036 + CF-060/083/086); OQ-002/OQ-003 resolved (ADR-0015/ADR-0026); no C0 ADR rests on provisional evidence; LCD battery applied to A3/A4/A5; `synthesis/l1-inputs.md` written; **WS-L1 ratified 2026-09-10** (ADR-0009 ratified, decision ADR-0050; not a gate criterion of the synthesis, completed before the phase-boundary commit) | pending (phase-boundary commit) |
| 2 | C1–C4 · D1–D5 · E1–E5 · F1 F2 · G1–G3 · H1–H7 · B3 B4 B5 · I3 I4 | **passed** (2026-09-10) | all 31 done with 94 ratified ADRs (ADR-0051…0144; 17 amended at synthesis with logged amendment sections) + 2 synthesis ADRs (ADR-0145/0146); L3/H1/H2/D1/D4 authority scheme consistent (CF-006 closed; ADR-0145 §A); effect path B2↔E5↔H1↔H4↔B5 composes (ADR-0145 §B); context and compilation chains compose (§C/§D) with the LCD battery applied to C3/E2; CF-005 resolved (ADR-0103/0106); no C0 ADR on provisional evidence; all Phase 2 conflicts dispositioned (CF-118…CF-321); Ontology **v2** | (orchestrator commit at phase boundary) |
| 3 | J1–J6 · K1–K4 · L5 | todo | all 11 done; thin observational ABI depth decided (J6); plugin contract (L5) consistent with J2 | — |
| 4 | F3 F4 F5 · I5 I6 I7 I8 · organizational layer (WS-L8, see below) | todo | all 8 done; every C4 decision carries matched-budget conditionality; no C0 decision rests on provisional evidence | — |
| 5 | Synthesis & Canonical Spec | todo | §7.2 skeleton authored; adversarial coherence review clean; `READINESS_REPORT.md` all-green | — |

## Workstream table

Fields per §11.2 step 4: `{id, track, phase, status, blocking-edges, dossier-path, owner-agent}`. `blocking-edges` = workstreams that must be `done` first. `owner-agent` is filled with the subagent label when fanned out.

### Track A — Foundations, Ontology & IR

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-A1 | Positioning & prior-art/competitive audit | A | 0 | **done** | — | research/dossiers/WS-A1.md | WS-A1 research subagent (Fable 5.1) | ADR-0003 (ratified, amended), ADR-0004 (ratified; amends ADR-0001 (b)), ADR-0005 (ratified) |
| WS-A2 | Novel ontology: 7-plane model, policy-stack formalism, validity vs compliance, compatibility surface, participant classes | A | 1 | **done** | A1, L7 | research/dossiers/WS-A2.md | WS-A2 research subagent (Fable 5.1) | ADR-0012 (ratified, amended), ADR-0013 (ratified), ADR-0014 (ratified, amended) |
| WS-A3 | Harness IR design: typed entities/edges; behavioral not framework-specific | A | 1 | **done** | A1 | research/dossiers/WS-A3.md | WS-A3 research subagent (Fable 5.1) | ADR-0015 (ratified), ADR-0016 (ratified, amended), ADR-0017 (ratified), ADR-0018 (ratified, amended) |
| WS-A4 | Compilation model: IR → runtime + model profiles + protocol targets | A | 1 | **done** | A1 | research/dossiers/WS-A4.md | WS-A4 research subagent (Fable 5.1; killed by usage limit after persisting) | ADR-0019 (ratified, amended), ADR-0020 (ratified), ADR-0021 (ratified, amended), ADR-0022 (ratified) |
| WS-A5 | Configuration & composition model; code-vs-config boundary | A | 1 | **done** | A1 | research/dossiers/WS-A5.md | WS-A5 research subagent (Fable 5.1) | ADR-0023 (ratified), ADR-0024 (ratified), ADR-0025 (ratified, amended) |

### Track B — Core Runtime & Durability

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-B1 | Event store / run ledger: taxonomy, ID model, materialized views | B | 1 | **done** | A1 | research/dossiers/WS-B1.md | WS-B1 research subagent (Fable 5.1) | ADR-0026 (ratified, amended), ADR-0027 (ratified, amended), ADR-0028 (ratified), ADR-0029 (ratified, amended) |
| WS-B2 | Effect & transaction model: intent/prepared/commit/observed; idempotency; leases; compensation | B | 1 | **done** | A1 | research/dossiers/WS-B2.md | WS-B2 research subagent (Fable 5.1) | ADR-0030 (ratified, amended), ADR-0031 (ratified, amended), ADR-0032 (ratified, amended) |
| WS-B3 | Durable execution & recovery | B | 2 | **done** | B1, B2 | research/dossiers/WS-B3.md | WS-B3 research subagent (Fable 5.1) | ADR-0130 (ratified), ADR-0131 (ratified), ADR-0132 (ratified) |
| WS-B4 | Reversible & speculative execution | B | 2 | **done** | B1, B2 | research/dossiers/WS-B4.md | WS-B4 research subagent (Fable 5.1) | ADR-0133 (ratified), ADR-0134 (ratified), ADR-0135 (ratified) |
| WS-B5 | Execution-environment abstraction | B | 2 | **done** | B1, B2, L3 | research/dossiers/WS-B5.md | WS-B5 research subagent (Fable 5.1) | ADR-0136 (ratified), ADR-0137 (ratified), ADR-0138 (ratified, amended) |

### Track C — Model Plane

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-C1 | Model adapter/gateway | C | 2 | **done** | A3, A4 | research/dossiers/WS-C1.md | WS-C1 research subagent (Fable 5.1) | ADR-0118 (ratified), ADR-0119 (ratified), ADR-0120 (ratified) |
| WS-C2 | Model router | C | 2 | **done** | A3, A4, L2 | research/dossiers/WS-C2.md | WS-C2 research subagent (Fable 5.1) | ADR-0121 (ratified, amended), ADR-0122 (ratified), ADR-0123 (ratified) |
| WS-C3 | Model-profile compiler | C | 2 | **done** | A3, A4 | research/dossiers/WS-C3.md | WS-C3 research subagent (Fable 5.1) | ADR-0124 (ratified, amended), ADR-0125 (ratified), ADR-0126 (ratified) |
| WS-C4 | Caching & token economics | C | 2 | **done** | L2 | research/dossiers/WS-C4.md | WS-C4 research subagent (Fable 5.1) | ADR-0127 (ratified), ADR-0128 (ratified), ADR-0129 (ratified) |

### Track D — Context & Memory

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-D1 | Context builder / policy engine | D | 2 | **done** | A3, L3, L2 | research/dossiers/WS-D1.md | WS-D1 research subagent (Fable 5.1) | ADR-0072 (ratified, amended), ADR-0073 (ratified, amended), ADR-0074 (ratified, amended) |
| WS-D2 | Compaction strategy family | D | 2 | **done** | A3, L2 | research/dossiers/WS-D2.md | WS-D2 research subagent (Fable 5.1) | ADR-0075 (ratified), ADR-0076 (ratified), ADR-0077 (ratified) |
| WS-D3 | Retrieval & memory hierarchy | D | 2 | **done** | A3, L3, B1 | research/dossiers/WS-D3.md | WS-D3 research subagent (Fable 5.1) | ADR-0078 (ratified), ADR-0079 (ratified), ADR-0080 (ratified, amended) |
| WS-D4 | Memory lifecycle semantics | D | 2 | **done** | A3, L3, L4 | research/dossiers/WS-D4.md | WS-D4 research subagent (Fable 5.1) | ADR-0081 (ratified, amended), ADR-0082 (ratified), ADR-0083 (ratified) |
| WS-D5 | Skills & Procedure IR | D | 2 | **done** | A3, A4 | research/dossiers/WS-D5.md | WS-D5 research subagent (Fable 5.1) | ADR-0084 (ratified), ADR-0085 (ratified), ADR-0086 (ratified) |

### Track E — Tools & Action

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-E1 | Tool registry & typed capabilities | E | 2 | **done** | A3, L3 | research/dossiers/WS-E1.md | WS-E1 research subagent (Fable 5.1) | ADR-0087 (ratified, amended), ADR-0088 (ratified, amended), ADR-0089 (ratified) |
| WS-E2 | Tool-interface compiler | E | 2 | **done** | A3, A4 | research/dossiers/WS-E2.md | WS-E2 research subagent (Fable 5.1) | ADR-0090 (ratified), ADR-0091 (ratified), ADR-0092 (ratified) |
| WS-E3 | Tool scaling: discovery, deferred loading | E | 2 | **done** | A3 | research/dossiers/WS-E3.md | WS-E3 research subagent (Fable 5.1) | ADR-0093 (ratified), ADR-0094 (ratified), ADR-0095 (ratified) |
| WS-E4 | Protocol edges: MCP, A2A, ACP | E | 2 | **done** | A3, A4 | research/dossiers/WS-E4.md | WS-E4 research subagent (Fable 5.1) | ADR-0096 (ratified), ADR-0097 (ratified), ADR-0098 (ratified), ADR-0099 (ratified) |
| WS-E5 | Sandboxed tool execution & effect capture | E | 2 | **done** | B2, L3 | research/dossiers/WS-E5.md | WS-E5 research subagent (Fable 5.1) | ADR-0100 (ratified), ADR-0101 (ratified), ADR-0102 (ratified) |

### Track F — Control & Orchestration

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-F1 | Control-strategy family | F | 2 | **done** | A3, B1 | research/dossiers/WS-F1.md | WS-F1 research subagent (Fable 5.1) | ADR-0103 (ratified, amended), ADR-0104 (ratified), ADR-0105 (ratified) |
| WS-F2 | Control envelope | F | 2 | **done** | B2, L2 | research/dossiers/WS-F2.md | WS-F2 research subagent (Fable 5.1) | ADR-0106 (ratified), ADR-0107 (ratified), ADR-0108 (ratified) |
| WS-F3 | Sub-agent orchestrator | F | 4 | todo | B3, B5, H1, L2 | research/dossiers/WS-F3.md | — | — |
| WS-F4 | Value-of-compute scheduler | F | 4 | todo | L2, I2, F3 | research/dossiers/WS-F4.md | — | — |
| WS-F5 | Multi-agent coordination & consistency | F | 4 | todo | B1, B2, F3 | research/dossiers/WS-F5.md | — | — |

### Track G — Verification

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-G1 | Verification fabric | G | 2 | **done** | A3, B1 | research/dossiers/WS-G1.md | WS-G1 research subagent (Fable 5.1) | ADR-0109 (ratified), ADR-0110 (ratified), ADR-0111 (ratified) |
| WS-G2 | Execution-alignment / belief-state reconciliation | G | 2 | **done** | B1, L3 | research/dossiers/WS-G2.md | WS-G2 research subagent (Fable 5.1) | ADR-0112 (ratified), ADR-0113 (ratified), ADR-0114 (ratified) |
| WS-G3 | Independent critics/evaluators | G | 2 | **done** | I2 | research/dossiers/WS-G3.md | WS-G3 research subagent (Fable 5.1) | ADR-0115 (ratified), ADR-0116 (ratified), ADR-0117 (ratified) |

### Track H — Security & Governance

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-H1 | Reference-monitor kernel & capability model | H | 2 | **done** | L3, A3 | research/dossiers/WS-H1.md | WS-H1 research subagent (Fable 5.1) | ADR-0051 (ratified, amended), ADR-0052 (ratified, amended), ADR-0053 (ratified) |
| WS-H2 | Information-flow control & taint labeling | H | 2 | **done** | L3 | research/dossiers/WS-H2.md | WS-H2 research subagent (Fable 5.1) | ADR-0054 (ratified), ADR-0055 (ratified), ADR-0056 (ratified) |
| WS-H3 | Credential mediation & secret isolation | H | 2 | **done** | L3 | research/dossiers/WS-H3.md | WS-H3 research subagent (Fable 5.1) | ADR-0057 (ratified), ADR-0058 (ratified), ADR-0059 (ratified) |
| WS-H4 | Egress/network policy & sandbox boundaries | H | 2 | **done** | B5 | research/dossiers/WS-H4.md | WS-H4 research subagent (Fable 5.1) | ADR-0060 (ratified), ADR-0061 (ratified, amended), ADR-0062 (ratified, amended) |
| WS-H5 | Extension supply-chain trust | H | 2 | **done** | L3, L4 | research/dossiers/WS-H5.md | WS-H5 research subagent (Fable 5.1) | ADR-0063 (ratified), ADR-0064 (ratified), ADR-0065 (ratified) |
| WS-H6 | Deep audit trail & tamper-evident logging | H | 2 | **done** | B1, I1 | research/dossiers/WS-H6.md | WS-H6 research subagent (Fable 5.1) | ADR-0066 (ratified, amended), ADR-0067 (ratified), ADR-0068 (ratified) |
| WS-H7 | HITL/approval & escalation policy | H | 2 | **done** | L2 | research/dossiers/WS-H7.md | WS-H7 research subagent (Fable 5.1) | ADR-0069 (ratified), ADR-0070 (ratified), ADR-0071 (ratified, amended) |

### Track I — Measurement & Evolution

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-I1 | Telemetry/tracing + cost & latency instrumentation | I | 1 | **done** | A1 | research/dossiers/WS-I1.md | WS-I1 research subagent (Fable 5.1) | ADR-0042 (ratified, amended), ADR-0043 (ratified, amended), ADR-0044 (ratified, amended) |
| WS-I2 | Eval framework: factorial design; 8-dim scorecard | I | 1 | **done** | A1 | research/dossiers/WS-I2.md | WS-I2 research subagent (Fable 5.1) | ADR-0045 (ratified), ADR-0046 (ratified, amended), ADR-0047 (ratified, amended) |
| WS-I3 | Reproducible harness bundle format | I | 2 | **done** | L4, I2 | research/dossiers/WS-I3.md | WS-I3 research subagent (Fable 5.1) | ADR-0139 (ratified), ADR-0140 (ratified), ADR-0141 (ratified) |
| WS-I4 | Benchmark/environment integration | I | 2 | **done** | I2, B5 | research/dossiers/WS-I4.md | WS-I4 research subagent (Fable 5.1) | ADR-0142 (ratified), ADR-0143 (ratified), ADR-0144 (ratified) |
| WS-I5 | Evolution service as governed pipeline | I | 4 | todo | I2, I3, H1, B4 | research/dossiers/WS-I5.md | — | ADR-0002 (pre-ratified scope) |
| WS-I6 | Assumption-debt manager | I | 4 | todo | C3, I2 | research/dossiers/WS-I6.md | — | — |
| WS-I7 | Causal attribution & counterfactual execution | I | 4 | todo | B4, I2 | research/dossiers/WS-I7.md | — | — |
| WS-I8 | Model-harness co-evolution & consolidation | I | 4 | todo | I5, I6 | research/dossiers/WS-I8.md | — | ADR-0002 (pre-ratified scope) |

### Track J — Meta-Harness Laboratory

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-J1 | Harness assembly & declarative definition | J | 3 | todo | A3, A5 | research/dossiers/WS-J1.md | — | — |
| WS-J2 | Component-variation registry | J | 3 | todo | A5, L4 | research/dossiers/WS-J2.md | — | — |
| WS-J3 | Experiment & sweep engine | J | 3 | todo | I2, L2, B5 | research/dossiers/WS-J3.md | — | — |
| WS-J4 | Comparison & analysis engine | J | 3 | todo | I2 | research/dossiers/WS-J4.md | — | — |
| WS-J5 | Results store, experiment ledger & leaderboard | J | 3 | todo | B1, I3, L4 | research/dossiers/WS-J5.md | — | — |
| WS-J6 | External-harness hosting / thin observational ABI | J | 3 | todo | A2, E4, I1, I2 | research/dossiers/WS-J6.md | — | ADR-0001 (pre-ratified positioning) |

### Track K — Surfaces

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-K1 | CLI design & command surface | K | 3 | todo | A5, J1 | research/dossiers/WS-K1.md | — | — |
| WS-K2 | Web dev-tooling | K | 3 | todo | B1, I1, J4 | research/dossiers/WS-K2.md | — | — |
| WS-K3 | MCP server exposing MetaHarness | K | 3 | todo | E4 | research/dossiers/WS-K3.md | — | — |
| WS-K4 | SDK/embedding API | K | 3 | todo | A4, A5 | research/dossiers/WS-K4.md | — | — |

### Track L — Cross-cutting & Program

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-L1 | Language/ecosystem selection spike (framed P0, ratified end-P1) | L | 0→1 | **done** (framing 2026-09-09; **ratification 2026-09-10** — ADR-0009 procedure executed; decision ADR-0050; spike spec `synthesis/l1-spike-spec.md` for Stage 0) | (framing: —) (ratify: A3, A4, A5, B1 + questionnaire OQ-040…046 answered) | research/dossiers/WS-L1.md | WS-L1 research subagent (Fable 5.1); WS-L1 ratification subagent (Fable 5.1) | ADR-0009 (**ratified**, amended 2026-09-10), **ADR-0050 (ratified)** |
| WS-L2 | Resource-economics & accounting model | L | 1 | **done** | A1 | research/dossiers/WS-L2.md | WS-L2 research subagent (Fable 5.1) | ADR-0039 (ratified, amended), ADR-0040 (ratified, amended), ADR-0041 (ratified) |
| WS-L3 | Provenance & authority model | L | 1 | **done** | A1 | research/dossiers/WS-L3.md | WS-L3 research subagent (Fable 5.1) | ADR-0033 (ratified), ADR-0034 (ratified), ADR-0035 (ratified, amended) |
| WS-L4 | Versioning, reproducibility & artifact identity | L | 1 | **done** | A1 | research/dossiers/WS-L4.md | WS-L4 research subagent (Fable 5.1) | ADR-0036 (ratified), ADR-0037 (ratified), ADR-0038 (ratified) |
| WS-L5 | Extensibility/plugin architecture & third-party component contracts | L | 3 | todo | A5, J2 | research/dossiers/WS-L5.md | — | — |
| WS-L6 | Packaging, licensing, OSS governance, docs & community | L | 5 | todo | L1 | research/dossiers/WS-L6.md | — | — |
| WS-L7 | Novelty/differentiation thesis & naming | L | 0 | **done** | — | research/dossiers/WS-L7.md | WS-L7 research subagent (Fable 5.1) | ADR-0006 (ratified, amended), ADR-0007 (ratified; `registers/lcd-test-battery.md`), ADR-0008 (ratified, amended) |
| WS-L8 | Human-agent organizational layer (§2.12, C4; Phase 4 "organizational layer" — no WS ID in §5, assigned here) | L | 4 | todo | F3, H7, B3 | research/dossiers/WS-L8.md | — | — |

**Count:** 52 workstreams (51 from §5 + WS-L8 assigned to the §6 Phase 4 "organizational layer" item, which §5 names but does not ID). WS-L6 is scheduled in Phase 5 (no §6 phase names it; it depends on WS-L1).

## ADR index

| ADR | Title | Status | Owner WS | Phase |
|---|---|---|---|---|
| ADR-0001 | Positioning: native-primary, hosting-as-participant; two participant classes | **ratified** (pre-ratified by sponsor) | WS-A1, WS-J6 | Preflight |
| ADR-0002 | Evolution / co-evolution fully in-scope, not stubbed | **ratified** (pre-ratified by sponsor) | WS-I5, WS-I8 | Preflight |
| ADR-0003 | Positioning: research instrument + reference runtime, not a general orchestration framework; non-goals (canonical list in ADR-0006); resolves CF-009 | **ratified** (amended: non-goal numbering unified, CF-019) | WS-A1 (WS-L7) | 0 |
| ADR-0004 | Amend ADR-0001 evidence (b): two-participant-class plane is precedented; novelty narrowed to white-box component axis + class-scoped metrics + declared/observed conformance | **ratified** (applied to ADR-0001) | WS-A1 | 0 |
| ADR-0005 | Interoperate, don't duplicate: interchange import/export; mechanism-neutral Hosting ABI (minimum observable event set + capability declaration); act as a participant in external planes | **ratified** | WS-A1 (J6, J5, I3, I4, B1, L2) | 0 |
| ADR-0006 | Differentiation thesis + canonical non-goals N1–N13 for Spec §1; narrowed, falsifiable novelty claims | **ratified** (amended: N1–N13; WS-A1 confirmation recorded) | WS-L7 (WS-A1) | 0 |
| ADR-0007 | LCD-trap test battery T-LCD-01…15 as binding acceptance criteria (`registers/lcd-test-battery.md`) | **ratified** | WS-L7 → A3/A4/A5/B1/I1/I2/C3/E2/E4/J3–J6/L5 | 0 |
| ADR-0008 | Canonical names: Harness IR (HIR), Harness Definition, native/hosted participant, Harness Lab, Hosting ABI, Model Profile/Profile Compiler, target/lowering/lifting, component class/variant, configuration/arm, class-scoped metric, capability declaration/vector; product name retained with disclosed collision | **ratified** (amended: effective at Ontology v0.1; CF-020/021 flagged to WS-A2) | WS-L7 (WS-A2, WS-L6) | 0 |
| ADR-0009 | WS-L1 language/ecosystem decision criteria, candidate classes E1–E5, ratification procedure (no language chosen) | **ratified** (end of Phase 1, 2026-09-10, together with ADR-0050; amendment log records procedure execution) | WS-L1 | 0→1 |
| ADR-0010 | Scope change: R-2.10.6 hosting MoSCoW Could → Should; cross-participant comparison delivered in two steps | **ratified** (synthesis-authored) | synthesis (WS-A1, WS-L7 → WS-J6) | 0 |

| ADR-0011 | Product name: HarnessHarness (sponsor decision; resolves OQ-035/CF-013) | **ratified** (sponsor, 2026-09-09) | WS-L7, WS-L6, WS-A2 | 0→1 boundary |

| ADR-0012 | Ontology v1: two levels, planes as a partition with a home-plane rule, two boundaries, control boundary β, compatibility surface as a derived view; vocabulary dispositions | **ratified, amended** | WS-A2 | 1 |
| ADR-0013 | Participant class = transparency of H; participant descriptor; admissible granularities; total ABI projection; `ledger ⇔ native` | **ratified** | WS-A2 | 1 |
| ADR-0014 | Validity vs compliance as a measured pair: three-valued validity; delivered/activated/followed chain with detector provenance; realized-benefit decomposition | **ratified, amended** | WS-A2 | 1 |
| ADR-0015 | HIR type discipline and expressiveness ceiling: closed kernel sums + registered `ext`; `Text`/`CompiledPayload` leaves; two content-addressed ids; canonical form (resolves OQ-002, OQ-040) | **ratified** | WS-A3 | 1 |
| ADR-0016 | HIR/1 entity and edge catalogue (13 entities + 2 leaves + 7 edges) with provenance and version records; kernel invariants; vocabulary arbitration | **ratified, amended (P2)** | WS-A3 | 1 |
| ADR-0017 | Typed diff (`HirDiff`) as the unit of harness edit | **ratified, amended (P2)** | WS-A3 | 1 |
| ADR-0018 | Hosted participants in the IR: `AgentProcess.hosted = OpaqueProcess`; Open Agent Spec as a declared-lossy C2 interop target (OQ-031) | **ratified, amended** | WS-A3 | 1 |
| ADR-0019 | Staged, pure, content-addressed compilation of HIR; runtime-interpreted plans; closed-world rule; re-lowering (OQ-043 part 1) | **ratified, amended (P2)** | WS-A4 | 1 |
| ADR-0020 | The Model Profile contract: selector, typed capability declaration, closed rule kinds, debt record on every rule, four test suites, expiry states; C0 schema + two minimal profiles | **ratified, amended (P2)** | WS-A4 | 1 |
| ADR-0021 | Protocol targets as schema-conformant data with a minimum carried set through typed extension slots; MCP C0/Stage 3, ACP C1/Stage 4, A2A/Agent Spec C2 (OQ-043 part 2) | **ratified, amended (P2)** | WS-A4 | 1 |
| ADR-0022 | Semantic-equivalence obligations E1–E7 for compiled tool surfaces; composite/synthesized surfaces C2 | **ratified, amended (P2)** | WS-A4 | 1 |
| ADR-0023 | A Harness Definition is a data document: assembly grammar; no host-language code in definitions; locality-neutral binding (OQ-044) | **ratified** | WS-A5 | 1 |
| ADR-0024 | Code-vs-config boundary: MUST-code/MUST-data lists, migration ladder, pointer rule, monotone authority layering with per-field merge policy | **ratified** | WS-A5 | 1 |
| ADR-0025 | Assembly validation contract and sealed-definition identity; one resolver feeding `link`; `HirDiff` as the edit unit; resume verification; parameter space + override grammar | **ratified, amended** | WS-A5 | 1 |
| ADR-0026 | The run ledger is the sole authoritative record; views derived; selective event sourcing; one fenced writer per run (resolves OQ-003, OQ-042 1–2); taxonomy amendments in its log | **ratified, amended (P2)** | WS-B1 | 1 |
| ADR-0027 | Ledger ID model, ordering and causality: allocated time-ordered ids, content addresses, per-run dense `seq`, parent/link causality, fork-by-reference | **ratified, amended** | WS-B1 | 1 |
| ADR-0028 | Durability = event reconstruction; deterministic replay is a declared variant capability (OQ-042 part 3) | **ratified** | WS-B1 | 1 |
| ADR-0029 | Canonical byte form for hashing, per-run hash chain, payload offload (OQ-041 for events) | **ratified, amended** | WS-B1 | 1 |
| ADR-0030 | Effect lifecycle as typed ledger events: write-ahead commit record, explicit `unknown`/`probe`, fencing inherited from the writer lease | **ratified, amended (P2)** | WS-B2 | 1 |
| ADR-0031 | Effect risk class (projection of `EffectAttributes`), per-class delivery semantics, idempotency-key derivation, monotone kernel consumption (OQ-015) | **ratified, amended (P2)** | WS-B2 | 1 |
| ADR-0032 | Compensation as an audited, idempotent, best-effort saga with an `abandoned` terminal state | **ratified, amended** | WS-B2 | 1 |
| ADR-0033 | One provenance/authority scheme: the provenance record, seven authority classes, product label lattice (answers OQ-054; resolves CF-006 at the scheme level) | **ratified** | WS-L3 | 1 |
| ADR-0034 | Label propagation: narrow-or-preserve; per-call context label; role-placement invariant; retrieval order validity → authority → readers → relevance | **ratified, amended (P2)** | WS-L3 | 1 |
| ADR-0035 | Endorsement/declassification points (closed basis list; the model never endorses); mandatory-provenance ledger table; seven-check minimal monitor set | **ratified, amended (P2)** | WS-L3 | 1 |
| ADR-0036 | Unified identity model: five identity kinds, self-describing digests under a pinned identity profile, rotation procedure; two configuration ids | **ratified, amended (P2)** | WS-L4 | 1 |
| ADR-0037 | Immutability, supersession and revocation over versioned records; names as append-only histories; sameness ladder L0–L4 | **ratified, amended (P2)** | WS-L4 | 1 |
| ADR-0038 | Reproducibility levels R0–R3; the bundle as a lock manifest; results-store and audit keys | **ratified, amended (P2)** | WS-L4 | 1 |
| ADR-0039 | Unified resource taxonomy and accounting contract: canonical token roles, charges with attribution, spend derived with provenance (OQ-033) | **ratified, amended (P2)** | WS-L2 | 1 |
| ADR-0040 | Hierarchical budget contract: budget nodes, slice/pool delegation, reserve-before-spend, root-first exhaustion, soft vs hard budgets | **ratified, amended (P2)** | WS-L2 | 1 |
| ADR-0041 | Matched-budget comparison definitions (matched-cap, iso-cost, matched-total), engine refusal, provenance-stratified Pareto reporting | **ratified, amended (P2)** | WS-L2 | 1 |
| ADR-0042 | The trace/span model is a projection of the run ledger; export is lowering; the ledger is never sampled | **ratified, amended** | WS-I1 | 1 |
| ADR-0043 | Cost and latency measurement contract: measurement points M1–M18, integer units, monotonic clocks, cost provenance, one token convention | **ratified, amended (P2)** | WS-I1 | 1 |
| ADR-0044 | Instrumentation levels, telemetry sink classification, process-metric catalogue as ledger-computed declarations | **ratified, amended (P2)** | WS-I1 | 1 |
| ADR-0045 | Scorecard and experimental-design contract: full `MetricDeclaration`; six factors + replicate axis; outcome classes; distributional reporting; `n/a{reason}` (OQ-037, OQ-046 Q-L1-13) | **ratified** | WS-I2 | 1 |
| ADR-0046 | Matched-budget paired comparison protocol: `eval_budget` equality precondition; `search_budget` record; artifact/search-time/transfer benefit kinds; C4 gates | **ratified, amended** | WS-I2 | 1 |
| ADR-0047 | Oracle taxonomy and judge governance: nine oracle classes; deterministic-only headline at C0; judged values labelled/calibrated/independent/instrument-charged; veto invariants | **ratified, amended (P2)** | WS-I2 | 1 |
| ADR-0048 | Phase 1 cross-dossier reconciliation: canonical identifiers and shared-contract rulings (CF-101…CF-115); OQ-052/070/073/079/088/126; scope stage notes | **ratified** (synthesis-authored) | Phase 1 synthesis | 1 |
| ADR-0049 | WS-L1 ratification inputs: no hard gate fires; OQ-045 provisional answer; Q-L1-11 tally; Q-L1-14 provisional; spike plan (OQ-047); language-leak audit clean | **ratified** (synthesis-authored; chooses no language) | Phase 1 synthesis for WS-L1 | 1 |
| ADR-0050 | **WS-L1 language/ecosystem decision:** polyglot split E5a with a late-bound surface layer — kernel/reference runtime + sandbox helper in the compiled memory-safe ecosystem (E1), laboratory in the dynamically-typed research ecosystem (E2) over a per-run subprocess + JSON-RPC stdio boundary with generated bindings (WS-L1 §6.5 contract, C0/Stage 1, WS-K4 + WS-L5), surfaces via generated client (E3 default, WS-K2/K4 bind at Phase 3); frozen weights, scored matrix, sweep (win share 0.746, E1-kernel family 0.979), tie-breakers T3/T4, per-layer reversibility, revalidation triggers, Phase ≥ 2 language-use constraint (resolves OQ-001; R-2.12.3) | **ratified, amended (P2)** (WS-L1 ratification pass, 2026-09-10) | WS-L1 (ratification) | 1 (end) |

| ADR-0051 | Authority handles: kernel-table object-capabilities derived from the sealed definition and the principal, never present in model context; conferral rule, scope over capability parameters, and the `… | **ratified, amended** | WS-H1 | 2 |
| ADR-0052 | The reference-monitor decision function: ordered checks over records, the policy table Π and its default rows (OQ-101), risk-assessment precedence with `unknown ⇒ irreversible` (OQ-086), unattended… | **ratified, amended** | WS-H1 | 2 |
| ADR-0053 | The reference-monitor TCB boundary (OQ-021), attenuating delegation for subagents, hosted participants and the evolution service, the self-modifying policy layer as an explicit threat actor, and pr… | **ratified** | WS-H1 | 2 |
| ADR-0054 | C2 propagation semantics per derivation type: prospective post-call label, admission, quarantine handle, label-seeded branches; the deterministic/advisory split (one lattice, two enforcement depths) | **ratified** | WS-H2 | 2 |
| ADR-0055 | The C2 declassification/endorsement contract: per-basis effects on `taint`/`readers`, capacity-bounded shape endorsement, bounded sanitizers as `policy_rule`, robust declassification (D-ROBUST), an… | **ratified** | WS-H2 | 2 |
| ADR-0056 | The flow-policy language: a closed, quantifier-free, non-recursive, total predicate language over labels × canonical arguments × effect projection; evaluation order; exact policy-edit classificatio… | **ratified** | WS-H2 | 2 |
| ADR-0057 | Secret-visibility invariants: credentials exist above the broker only as secret channels (`SecretRef`), never as values in definitions, contexts, ledger, environments or sinks | **ratified** | WS-H3 | 2 |
| ADR-0058 | The credential broker contract: register/grant/bind/mediate/mint/revoke/rotate; broker delivers, reference monitor decides; proxy-injection primary, minted scoped token secondary, wrapped long-live… | **ratified** | WS-H3 | 2 |
| ADR-0059 | The leak-test battery LT-01…LT-12 as class-scoped `veto` metrics with canary channels, and the default policy row for `secret_access` | **ratified** | WS-H3 | 2 |
| ADR-0060 | Containment policy as typed, provenance-bearing data attached to the environment handle (`ContainmentPolicy/1`; kernel-protected paths; layered meet by authority class) | **ratified** | WS-H4 | 2 |
| ADR-0061 | Egress mediation and observation contract: `decide_egress` with a fixed evaluation order, DNS as egress, attribution by `effect_id`, credential-binding anti-laundering, and `security.egress.*` / `s… | **ratified, amended** | WS-H4 | 2 |
| ADR-0062 | Deterministic containment is the floor beneath the capability ceiling: enforcement points and their trust, `enforcement_evidence` and fail-closed (`ContainmentUnverified`), the escape hatch as an e… | **ratified, amended** | WS-H4 | 2 |
| ADR-0063 | The extension trust model: three-leg trust record, provenance class on injection, location neutrality, attestation → `pin`, and execution isolation for skills, hooks, plugins, MCP servers and instr… | **ratified** | WS-H5 | 2 |
| ADR-0064 | The extension lifecycle: declared sources instead of run-time discovery, pinning and surface pins at `resolve`, transparent `review`, update as supersession, revocation with propagation into live r… | **ratified** | WS-H5 | 2 |
| ADR-0065 | The model-install path: agent-initiated extension installation as a kernel Procedure of ordinary effects, approval of one install effect, attenuated and tainted results, and a non-widening definiti… | **ratified** | WS-H5 | 2 |
| ADR-0066 | The audit trail is the run ledger read through `audit_view`: the audit-grade event catalogue with content-free envelopes, kernel-only producers and declared obligations | **ratified, amended** | WS-H6 | 2 |
| ADR-0067 | Tamper-evidence contract: per-run chain, compact-range tree heads, kernel-signed checkpoints, inclusion/consistency proofs, cross-run anchors and independent auditors; witnesses and receiver receip… | **ratified** | WS-H6 | 2 |
| ADR-0068 | Redaction, garbage collection and retention over an audited ledger (envelopes never leave; content leaves only by endorsed tombstone; ordered retention floors), and `audit_completeness` as a compon… | **ratified** | WS-H6 | 2 |
| ADR-0069 | Escalation policy model: closed escalation input, Π floor rows, the never-auto set, a narrowing-only reviewer chain, and attendance-defined resolution of `ask` | **ratified** | WS-H7 | 2 |
| ADR-0070 | Approval request/response contract: per-effect approval records, durable `pending` vs ephemeral `requested`, decision-before-`prepared`, refusal semantics, asynchronous `defer`, batching, approver… | **ratified** | WS-H7 | 2 |
| ADR-0071 | Approval leases and approvals as a budgeted resource: the lease key (OQ-091), `approvals.requested` as the budgeted dimension (OQ-096) with an exhaustion safeguard, repeated-denial fallback, typed… | **ratified, amended** | WS-H7 | 2 |
| ADR-0072 | The context builder is kernel admission plus a pluggable selection policy over a content-addressed context plan; admission invariants are C0 and the policy can never widen authority | **ratified, amended** | WS-D1 | 2 |
| ADR-0073 | Context budget discipline and offloading: required/optional retention with kernel priority eviction, reserve-before-select, whole-item omission with a kernel notice, `ContextWindowExceeded → Compac… | **ratified, amended** | WS-D1 | 2 |
| ADR-0074 | Layout, ordering and rendering ownership: six kernel-reserved slots with fixed minimum authority; profile-owned slots, precedence, comparators, cache tiers and reminders as conditioned rules; the b… | **ratified, amended** | WS-D1 | 2 |
| ADR-0075 | Compaction is one component class over a closed operation sum: the `assess / propose / execute / declare` contract, hard/soft requirements, kernel invariants, and a fallback ladder ending in the C0… | **ratified** | WS-D2 | 2 |
| ADR-0076 | Provenance and validity of compaction derivations, and the three homes of model-conditioned compaction: summaries are `delegate`-derived and never authoritative; offloads and deterministic restruct… | **ratified** | WS-D2 | 2 |
| ADR-0077 | The canonical laboratory comparison `lab/compaction-family-v1`: compaction strategy as a component-level factor under matched cap with compaction charged to the subject; ledger-computed process met… | **ratified** | WS-D2 | 2 |
| ADR-0078 | The memory hierarchy is five addressing classes over three ratified substrates; one `MemoryStore` contract; derived memories are immutable provenance-bearing records distinct from materialized view… | **ratified** | WS-D3 | 2 |
| ADR-0079 | The retrieval contract: a closed query-kind sum (deterministic kinds at C0/C1, similarity at C2), the ADR-0034 filter order applied before a ranker component class that cannot widen, budgeted whole… | **ratified** | WS-D3 | 2 |
| ADR-0080 | Memory writes and consolidation: agent-authored memories are immutable versions in a runtime-owned memory store with a `delegate` write ceiling and a `≤ external` label cap (supersedes one row of A… | **ratified, amended** | WS-D3 | 2 |
| ADR-0081 | Memory lifecycle as derived state over immutable versions: the invalidation contract (closed dependency kinds, cache hints, freshness, validators, minimum-contract rule), three kernel enforcement p… | **ratified, amended** | WS-D4 | 2 |
| ADR-0082 | Supersession, revocation and conflict sets for memory: the authority rule, closed revocation reasons, deterministic-core/judged-tail conflict detection, no recency tie-break, slot conflict policy,… | **ratified** | WS-D4 | 2 |
| ADR-0083 | Downstream inheritance of invalidation via kernel-stamped memory justifications and a derived `MemoryStaleIndex` (OQ-020, retrieval half); memory validity-vs-compliance metrics and veto invariants… | **ratified** | WS-D4 | 2 |
| ADR-0084 | The Procedure/1 profile: HIR/1 `Procedure` extended only through a registered `ext` block; checkable preconditions vs applicability notes; tests through `validates` edges; model-free procedure vali… | **ratified** | WS-D5 | 2 |
| ADR-0085 | Three compilation targets for a Procedure (`instruction`, `workflow_node`, `subagent_task`), a total selection rule from bindability, risk and isolation, feasibility errors, the `procedure_target`… | **ratified** | WS-D5 | 2 |
| ADR-0086 | Skills are lifted Procedures: the Agent Skills file-tree mapping in both directions with a loss report, render purity, claims-to-grants only at `seal`, the procedure lifecycle and promotion policy,… | **ratified** | WS-D5 | 2 |
| ADR-0087 | The capability record: `ToolCapability` semantic/surface field discipline, closed precondition domains and scope bindings, observation contract, and no second risk declaration | **ratified, amended** | WS-E1 | 2 |
| ADR-0088 | The tool registry contract: immutable content-addressed capability versions, source-kind lifting with `unverified` provenance, refresh-as-supersession (rug-pull defence), snapshots, binding and cat… | **ratified, amended** | WS-E1 | 2 |
| ADR-0089 | Cost, risk, observability and exposure metadata on capabilities: declared cost is advisory and never accounting; risk is derived only; observability is a content-class declaration lowered to standa… | **ratified** | WS-E1 | 2 |
| ADR-0090 | The tool-interface compiler contract: closed exposure modes, surface families with a reference variant, declared task/environment inputs, `SurfaceBinding` attribution, a closed `SurfaceArgMap.trans… | **ratified** | WS-E2 | 2 |
| ADR-0091 | Wrapper synthesis and admission: a gated pipeline over ADR-0022's E1–E7 plus safety obligations S1–S4; synthesized surfaces enter a bundle only as conditioned rules with assumption-debt records; pe… | **ratified** | WS-E2 | 2 |
| ADR-0092 | Result and failure rendering are compiled, validator-aware surface records: `ResultRenderSpec` with declared loss and offload-to-artifact (compression never a default; token-optimized notations ina… | **ratified** | WS-E2 | 2 |
| ADR-0093 | Exposure modes, the exposure plan and `callable ⇔ revealed`: the tool plane's run-time selection under the closed-world rule | **ratified** | WS-E3 | 2 |
| ADR-0094 | The discovery capability and the `catalog_index` component class: one capability, two lowerings, an offline retrieval suite, and class-scoped selection metrics | **ratified** | WS-E3 | 2 |
| ADR-0095 | Catalog sync and epochs: catalog drift as ledger data, `freeze | adopt | ask`, adoption as incremental lowering with compiled identities | **ratified** | WS-E3 | 2 |
| ADR-0096 | Boundary placement for the protocol edges: MCP is the sole standardized tool-supply channel, ACP is the session boundary, A2A is the delegation edge; orchestration, safety, memory, budgets and prov… | **ratified** | WS-E4 | 2 |
| ADR-0097 | The MCP edge under the 2026-07-28 revision: a stateless canonical tool catalogue per bundle (amends ADR-0021 decision 5), a dual-era client with probe-first negotiation, `input_required` as a pause… | **ratified** | WS-E4 | 2 |
| ADR-0098 | The ACP edge: a native harness as an ACP v2 agent with a v1 compatibility profile and an exhaustive event-class lowering table; permission transport; `_hh/*` extension methods; the Hosting ABI's se… | **ratified** | WS-E4 | 2 |
| ADR-0099 | Protocol version and capability negotiation, and trust at every edge: the `ProtocolBinding` record, probe-first pinning, typed `ProtocolVersionMismatch`, omitted → `unknown`, unknown preservation,… | **ratified** | WS-E4 | 2 |
| ADR-0100 | The execution contract: seven kernel-owned stages over the ADR-0030 lifecycle, the `tool_executor` component contract ("executors report, never decide"), and the sandbox helper as a separate binary… | **ratified** | WS-E5 | 2 |
| ADR-0101 | Effect capture and attribution: a closed `CaptureItem` sum, the content-addressed `EffectCaptureManifest` with explicit completeness and source, a kernel-minted attribution token, redaction inside… | **ratified** | WS-E5 | 2 |
| ADR-0102 | Result normalization, the three-origin error taxonomy with kernel-derived retryability, the deadline hierarchy with helper-side enforcement, two-phase class-aware interruption, ephemeral streaming… | **ratified** | WS-E5 | 2 |
| ADR-0103 | Control lives behind one pluggable `control_strategy` contract inside a code-owned envelope (resolves CF-005 as "pluggable + envelope") | **ratified, amended** | WS-F1 | 2 |
| ADR-0104 | The Stage-0 minimal control strategy `react/minimal` (mini-SWE-agent-class; T-LCD-03 anchor) | **ratified** | WS-F1 | 2 |
| ADR-0105 | The control-strategy family is the Harness Lab's canonical component-level comparison; the control boundary moves only by `ComparisonReport` | **ratified** | WS-F1 | 2 |
| ADR-0106 | The control envelope is Core kernel code interpreting one sealed `EnvelopePolicy` record: six enforcement points on the harness step, a closed `StopReason` sum with outcome-class projection, and st… | **ratified** | WS-F2 | 2 |
| ADR-0107 | Retry and timeout policy: `RetryPolicy` keyed by `(scope_kind, error_class)` over ADR-0031 delivery semantics, shared identity across attempts with a recorded `attempt_delta`, `TimeoutPolicy` per s… | **ratified** | WS-F2 | 2 |
| ADR-0108 | Loop detection, output validation and the mandatory state-invariant set: semantic-keyed deterministic detectors with a nudge → deny → stop ladder (judged tier C1), `OutputValidationPolicy` with a b… | **ratified** | WS-F2 | 2 |
| ADR-0109 | Task contracts as a typed projection over `Goal` and `validates` records with visible and held-out criteria; a kernel completion gate; pinned validators and evidence-integrity veto predicates | **ratified** | WS-G1 | 2 |
| ADR-0110 | The Validator component contract: authoritative evidence handles in, typed verdicts with provenance and an inputs digest out; annotate-or-reject, never rewrite; three bindings (subject, contract, i… | **ratified** | WS-G1 | 2 |
| ADR-0111 | Check placement: kernel local checks after every effect terminal event, declared postconditions bound to capabilities or rules, global monitors at `verify`/`stop`; and the deterministic `followed`… | **ratified** | WS-G1 | 2 |
| ADR-0112 | Claims are typed `model_claim` Observations reconciled against authoritative handles: the claim model, the claim-ledger substrate, four-valued agreement, and the divergence taxonomy D1–D10 with det… | **ratified** | WS-G2 | 2 |
| ADR-0113 | The completion gate and intervention policy Γ: non-terminal effects block completion, `abandoned` annotates with a veto (OQ-092), holds are budget-bounded, honest failure passes, and parsed/judged… | **ratified** | WS-G2 | 2 |
| ADR-0114 | Execution-alignment metrics as class-scoped `MetricDeclaration`s in the grounding dimension; `false_completion_rate` is veto-eligible; divergence severities are owned by WS-G2 (OQ-125); the reconci… | **ratified** | WS-G2 | 2 |
| ADR-0115 | The critic contract: critics are `Validator` variants over a kernel-built evidence bundle with a closed evidence-class set; verdict grounding is derived from cited evidence; verdict authority follo… | **ratified** | WS-G3 | 2 |
| ADR-0116 | Critic independence and capability isolation: a six-axis `IndependenceVector` with per-use minimums; critics run as attenuated read-only processes with their own budgets under declared evidence-adm… | **ratified** | WS-G3 | 2 |
| ADR-0117 | Critic calibration and cost governance: the `CalibrationRecord` with expiry as assumption debt (resolves OQ-123's shape); balanced-position and panel rules; exploratory status without calibration;… | **ratified** | WS-G3 | 2 |
| ADR-0118 | The model gateway is a model-blind, dialect-parameterised transport component: it consumes the compiled provider request plan as data, yields a normalized model-event grammar with opaque owner-stam… | **ratified** | WS-C1 | 2 |
| ADR-0119 | Closed stop-reason and error-class vocabularies with retry classes; `WireDialect` descriptors as expirable data with assumption-debt records; attempt policy as data; identical-bytes transport attem… | **ratified** | WS-C1 | 2 |
| ADR-0120 | Capability discovery and drift at the model boundary: tri-state transport capability declarations that are probed, discovery results as `external`-authority claims with pinned-profile precedence, s… | **ratified** | WS-C1 | 2 |
| ADR-0121 | The router is a policy component at the model boundary: role table as the unit of routing, typed routing request/decision, closed policy kinds as data with assumption-debt records, a composed compa… | **ratified, amended** | WS-C2 | 2 |
| ADR-0122 | Retry versus reroute: attempt-level retry owned by the control envelope, target-level reroute owned by the router; a closed error-class → action table; compaction-then-relower ordering; health/cool… | **ratified** | WS-C2 | 2 |
| ADR-0123 | Ensembles are budgeted control-plane procedures, not router features: `EnsembleProcedure{sample_k_vote, mixture_of_agents, cascade}` at the `delegate` decision point, k ledgered model calls each ro… | **ratified** | WS-C2 | 2 |
| ADR-0124 | The profile rule inventory `ProfileRuleInventory/1`, the capability/constraint split, additive `ModelProfile/1` fields, the Profile Compiler as a boundary component class, chain resolution with a c… | **ratified, amended** | WS-C3 | 2 |
| ADR-0125 | The profile test contract: validity (V1–V9 with golden surfaces and a null profile) vs conformance (probe specs, records, cadence; DRIFT refuses only on dependency capabilities) vs compliance (per-… | **ratified** | WS-C3 | 2 |
| ADR-0126 | The expiry/revalidation contract: observable-based expiry triggers, the profile status state machine as ledger events, compatibility-token change as a re-lowering trigger, the removal test as a mat… | **ratified** | WS-C3 | 2 |
| ADR-0127 | Cache taxonomy and declared cache semantics; prefix-stable layout constraints as kernel invariants; the `caching_markers` rule parameters | **ratified** | WS-C4 | 2 |
| ADR-0128 | Prefix affinity key, expected-vs-observed cache state, cache accounting events and metrics, and cache warmth as a mandatory Lab factor (answers OQ-097) | **ratified** | WS-C4 | 2 |
| ADR-0129 | Harness-level caches with validity semantics: tool-result cache as a memory kind with dependency stamps; response cache admissible only for reproduction and declared replay; similarity-keyed semant… | **ratified** | WS-C4 | 2 |
| ADR-0130 | The durability contract: checkpoint as a projection, phase-resolved restore, replay rules, and four fenced lease scopes | **ratified** | WS-B3 | 2 |
| ADR-0131 | Wakeups, suspended runs and long-lived goals: subscription → occurrence → claim → fire; `lifecycle.run.suspended`; goal activations chained by `continued_from` | **ratified** | WS-B3 | 2 |
| ADR-0132 | Self-healing environments by declared policy, and the kill-point failure-injection battery as the one shared durability acceptance suite | **ratified** | WS-B3 | 2 |
| ADR-0133 | Branch model and the fork / snapshot / rollback contract: two branch granularities, coherent fork points, atomic trace–environment coupling with declared coverage, rollback as an appended record | **ratified** | WS-B4 | 2 |
| ADR-0134 | Speculative-effect containment: risk-class-driven execution or deferral inside branches, the `deferred` phase, promotion-time re-authorization, discard as restore-plus-saga, `SpeculationPolicy` as… | **ratified** | WS-B4 | 2 |
| ADR-0135 | Nondeterminism recording rules and replay validity: the source table, four validity modes, the `ReplayValidityReport`, the intervention record, and the mandatory factual re-execution arm for counte… | **ratified** | WS-B4 | 2 |
| ADR-0136 | The environment handle: harness-outside topology, one environment protocol over three backend placements (OQ-045 final, B5 half), environment classes, the kernel-owned handle record with its lifecy… | **ratified** | WS-B5 | 2 |
| ADR-0137 | Environment identity and snapshot semantics: three identity layers (`EnvironmentRecord` as the environment factor, `EnvHandleId` as the instance, content-addressed `SnapshotRecord` as the state), a… | **ratified** | WS-B5 | 2 |
| ADR-0138 | The effect-operation surface (closed, effect-domain-tagged, no side door), workspace derivation for subagents / verifiers / replicates / branches, and environment accounting (`env.reserved/active/s… | **ratified, amended** | WS-B5 | 2 |
| ADR-0139 | The reproducible harness bundle is a three-layer content-addressed object: a hashed lock manifest, a content-addressed payload tree, and a detached sidecar; identity is the tree rule and archives a… | **ratified** | WS-I3 | 2 |
| ADR-0140 | Bundle validation and reproducibility-level contract: staged `validate_bundle` with complete-vs-valid status, a basis-derived `max_supported_level`, per-level `reproduce` procedures, and `ReproRepo… | **ratified** | WS-I3 | 2 |
| ADR-0141 | Bundle lifecycle and interop: status by supersession, publication policy with contamination defaults, export/import as lowering/lifting with loss reports, results-store linkage as a derived annotat… | **ratified** | WS-I3 | 2 |
| ADR-0142 | Environment families, the three-surface task record and the `benchmark_adapter` component contract (integrate, never author) | **ratified** | WS-I4 | 2 |
| ADR-0143 | Anti-leakage rules: benchmark policy never enters the harness (held-out surface never delivered; benchmark-conditioned rules refused at seal; split assignment before search; agent-phase egress `non… | **ratified** | WS-I4 | 2 |
| ADR-0144 | Grader integration and the Stage 3 suite: separate-verifier default, typed rewards with outcome classes, anti-gaming veto predicates, per-suite oracle declarations; a stratified, validity-annotated… | **ratified** | WS-I4 | 2 |
| ADR-0145 | Phase 2 cross-dossier reconciliation: effect path, context chain, compilation chain, CONTROL/verification seams; canonical spellings; amendments to 21 ratified Phase 1 ADRs (CF-118…CF-321 rulings) | **ratified** (synthesis-authored) | Phase 2 synthesis | 2 |
| ADR-0146 | Scope-register changes at Phase 2: R-2.7.2 split (R-2.7.2a/b); stage notes on R-2.2.3, R-2.3.3, R-2.4.2, R-2.4.5, R-2.5.2, R-2.7.1, R-2.8.5, R-2.9.3; SWE-bench Verified demotion recorded; 31 items → specified-by-ADR | **ratified** (synthesis-authored) | Phase 2 synthesis | 2 |

*(Workstreams append `ADR-NNNN` rows with `status: proposed`; synthesis passes ratify/amend/reject. Next number: **ADR-0147**.)*

## Phase progress log

### Preflight — 2026-09-09
- Docs 1–3 read in full by the orchestrator.
- `git init`; initial commit of corpus.
- Created `research/{dossiers,decisions,registers}` and `spec/sections`.
- Seeded `registers/sources.md` (S-001…S-131) from doc 2 Sources + all footnotes + Tier 0–4 syllabus.
- Initialized this LEDGER (52 workstreams), `open-questions.md`, `ontology.md` (v0), `conflicts.md`, `scope.md` (§2 imported with MoSCoW), `risks.md` (§9 imported).
- Wrote ADR-0001 and ADR-0002 as `ratified`.

### Phase 0 — 2026-09-09
- **Gate: passed.** WS-A1 and WS-L7 `done` with three ratified ADRs each; WS-L1 framed only (ADR-0009 stays `proposed`, ratify at end of Phase 1); Ontology v0 → **v0.1**; Source Registry folded (35 new sources S-132…S-166, 31 rows promoted S→P; S-055 unreachable, substituted by S-107 source); CF-009 resolved; language-leak audit clean (CF-025). Synthesis memo: `research/synthesis/phase-0.md`.
- **Positioning settled (ADR-0003, ADR-0006):** MetaHarness is a *research instrument + reference runtime*, not a general orchestration framework; "framework" is reserved for what others build on top. One canonical non-goals list **N1–N13** (two dossiers' lists merged, CF-019).
- **ADR-0001's `novel` flag refuted and narrowed (ADR-0004; ADR-0001 (b) amended, decision unchanged):** the two-participant-class comparison plane is precedented (Harbor, HAL [ICLR 2026], Inspect AI, Omnigent, Harness-Bench). What is novel: the component-decomposable white-box axis over the Harness IR, `MetricDeclaration` class/observability scoping (`n/a` never 0), and declared-vs-observed conformance (DRIFT) as data.
- **Doc 3 §3.4 novelty claim (i) narrowed and made conditional (CF-011/CF-015):** "first IR spanning all seven planes with provenance on every entity and profile-compiled lowering" — reverts to "renamed" if WS-A3 does not deliver Permission/Effect/Validator/Budget/Memory-validity/HarnessRule entities with provenance, or WS-C3/E2 do not deliver testable, expirable profiles. Proven, not asserted, by T-LCD-03/-04 at Stage 3.
- **Hosting (ADR-0005, ADR-0010):** Hosting ABI = minimum observable event set + capability declaration, neutral across session-ABI / model-boundary interception / container-installed mechanisms (session-ABI first); native ledger + interchange import/export; MetaHarness must be able to act as a participant in Harbor/Inspect/Omnigent-style planes (C2/Stage 4). R-2.10.6 MoSCoW Could → **Should**; two-step delivery (native-vs-native Stage 3, hosted Stage 4). OQ-004 reframed.
- **LCD-trap battery ratified (ADR-0007):** T-LCD-01…15 are binding acceptance criteria, published as `registers/lcd-test-battery.md`; static tests C0/Stage 1, contract tests Stages 1–3, executable tests Stage 3 (before profiles/evolution), T-07 with hosting (C2). `lcd_report` / `UnexpressibleSurface` (error, never warning). Assumption-debt *record schema* is C0/Stage 1 (scope note on R-2.9.6).
- **Vocabulary ratified at Ontology v0.1 (ADR-0008):** Harness IR (HIR) / Harness Definition / native & hosted participant / Harness Lab / Hosting ABI (never "Harness ABI") / Model Profile & Profile Compiler / target, lowering, lifting, lowering loss report / component class & variant / configuration & arm / class-scoped metric / capability declaration & capability vector / LCD trap, escape hatch, surface vs semantic identity / hosting mechanism, observability level, comparison granularity, observed conformance & drift, trajectory interchange, reference runtime, research instrument. Flagged to WS-A2: CF-020 (declaration vs vector), CF-021 (`configuration` enum value); opacity ratio pending OQ-036. No sub-concept uses "meta-".
- **Product name:** "MetaHarness" retained with four disclosed collisions (Omnigent self-description, ruvnet, SuperagenticAI, Stanford Meta-Harness); qualifier "MetaHarness — harness laboratory & reference runtime" recommended; sponsor decides via OQ-035 (WS-L6, Phase 5).
- **Language decision (WS-L1):** framed, not decided. Criteria C1–C12 with weight ranges, hard gates (C2; C4/C6 conditional), candidate classes E1–E5 incl. polyglot split, blind-weight procedure with sensitivity sweep. **Phase 1 dossiers must answer the questionnaire language-neutrally:** OQ-040 (A3), OQ-041 (A3+B1), OQ-042 (B1), OQ-043 (A4), OQ-044 (A5), OQ-045 (B5/E5 provisional via synthesis), OQ-046 (A1/I2/K4). Ratification is postponed by rule if any answer is missing.
- **Contracts converged for Phase 1:** WS-B1 must stamp `participant_class` + `observability_level` on every ledger event and add artefact *delivered/activated/followed* events (T-LCD-13); WS-A3 identity derivation excludes surface fields (T-LCD-10), free text only as typed provenance-bearing leaves (T-LCD-02), no per-model branches (T-LCD-01), no dependency edge into the Hosting ABI (T-LCD-06); WS-A4 emits lowering loss reports and raises `UnexpressibleSurface`; WS-A5/L5 contracts are operations not inheritance and receive the profile (T-LCD-08/-12); WS-I2 models factors + interaction effects, refuses un-budgeted arms, carries `applies_to` (T-LCD-09/-14/-15); WS-K4/L5 treat WS-L1 §6.5 boundary contract as C0/Stage-1 input (CF-023).
- **Evidence discipline check:** every ratified ADR carries §3.3 (a)–(e); no C0 decision rests on `provisional` evidence — 2026 preprints (S-062/S-072/S-088/S-148/S-133/S-074) are used for mechanism only and corroborated by Tier-A (S-135) / Tier-B source at pinned commits.
- **Open for later phases:** OQ-030 interchange format (J6), OQ-031 Agent Spec export (A3/A4), OQ-033 cost attribution per mechanism (L2/J4), OQ-034 DRIFT exclude-vs-annotate (J6/I6), OQ-036 opacity-ratio definition (A3/I2), OQ-037 gates vs checklists (P1 synthesis), OQ-038 grey-box interposition (J6/C1), OQ-039 T-LCD-03 anchor harness (A3/I4), OQ-047 spike execution (P1 synthesis). CF-017 (runtime-assumption exposure in B3/K2/E5/K4) stays pre-registered for Phase 2.
- **Registers:** sources next S-167 · open-questions next OQ-048 · conflicts next CF-026 · ADR next ADR-0012.
- **Next phase (1):** fan out WS-A2 A3 A4 A5 · B1 B2 · L2 L3 L4 · I1 I2 with the "settled for Phase 1" section of `research/synthesis/phase-0.md` in every brief; end-of-Phase-1 synthesis runs the WS-L1 ratification (ADR-0009) only after A3/A4/A5/B1 ADRs are ratified and OQ-040…046 answered.

### Phase 1 — 2026-09-10
- **Gate: passed.** All eleven Phase 1 workstreams `done`; 37 proposed ADRs dispositioned — 36 ratified (27 amended at synthesis with logged amendment sections), 0 rejected, plus two synthesis-authored ADRs (ADR-0048 reconciliation, ADR-0049 WS-L1 inputs). Synthesis memo: `research/synthesis/phase-1.md`; WS-L1 input: `research/synthesis/l1-inputs.md`.
- **Ontology v1** (`registers/ontology.md`, ADR-0012/0013/0014): §1–§4 replaced by the WS-A2 formal model (two levels; planes as a partition with a total `home` rule; model and environment *boundaries*, not an eighth plane; control boundary β over the closed `DecisionPoint` sum; validity three-valued; compliance = delivered → activated → followed with detector provenance; compatibility surface = derived view persisted only as a fitted-surface report; participant class = transparency of H, declared, `ledger ⇔ native`); 192 Phase 1 terms folded into §5a; §6 gains the Phase 1 canonical names (run ledger, sealed definition, `model_call_id`/`tool_call_id`, `context.artefact.*`/`verification.artefact.followed`, `configuration_id`/`configuration_version_id`, effect class vs effect risk class, the seven authority classes, `charged_to`, `Money`, `CostProvenance`).
- **HIR/1 (ADR-0015…0018):** closed kernel sums + registered `ext`; exactly two opaque leaf kinds (`Text`, `CompiledPayload`) with declared interface/owner/authority; `semantic_id`/`version_id` over a JCS-class canonical form; 13 entities + 2 leaves + 7 edges with a provenance record on every node and edge; `HirDiff` as the edit unit (invertible, classified, `authority_delta = widening` refused from non-human origins); `AgentProcess = {native | hosted = OpaqueProcess}`; Open Agent Spec is a declared-lossy C2 target. **OQ-002 resolved.** No C6 hard gate fires.
- **Compilation and composition (ADR-0019…0025):** staged pure compiler (`accept → link → lower-native → lower-profile → lower-target → seal`) emitting runtime-interpreted `RuntimePlan/1` and content-addressed bundles with a total trace map, closed-world rule and re-lowering event; Model Profile contract with a debt record on every rule and four test suites (C0 schema + two minimal profiles); protocol targets as schema-conformant documents with a minimum carried set and typed loss reports (MCP C0/Stage 3, ACP C1, A2A/Agent Spec C2); E1–E7 equivalence obligations; a Harness Definition is a data document (class-keyed `slots`, declared parameter space, profile unbound by default, code only as `CompiledPayload` or code pointers); MUST-code/MUST-data lists, migration ladder, monotone authority caps; one resolver (`compose → resolve → validate_assembly → seal`) feeding `link`. **OQ-043/OQ-044 answered.**
- **Run ledger and effects (ADR-0026…0032):** the typed append-only ledger + blobs + manifest is the sole authoritative record with `project()` views (**OQ-003 resolved**); one fenced writer per run, durable-before-visible; allocated time-ordered ids + content addresses + surface-id aliases, per-run dense `seq`, fork-by-reference; durability = reconstruction, deterministic replay a declared variant capability (**OQ-042 resolved**; C3 relaxed); canonical form + per-run hash chain + offload above 64 KiB (**OQ-041 with ADR-0015/0036**); effect lifecycle with write-ahead `committed` record and explicit `unknown`/`probe`; effect risk class as a projection of `EffectAttributes`; idempotency key from ledger state; compensation as an audited saga with `abandoned` (**OQ-015 resolved**).
- **Authority scheme converged (ADR-0033/0034/0035; CF-006 resolved at the scheme level):** one provenance record `{origin, authority, taint, readers, scope, derived_from, created_at, attestation?}`; `AuthorityClass = kernel > definition > principal > delegate > environment > external > unverified` (join = min); product label lattice, `authority` enforced at C0, `taint`/`readers` at C2; narrow-or-preserve propagation; per-call context label; role-placement invariant (roles are profile surfaces); retrieval order validity → authority → readers → relevance (CF-008); closed endorsement basis list — a `delegate` origin can never endorse; seven-check minimal monitor set. HIR/1, the ledger envelope (mandatory-provenance table) and the trace model all carry this one record.
- **Identity model converged (ADR-0036/0037/0038 with ADR-0015/0027/0029):** five identity kinds under a pinned, rotatable identity profile (`idp/1`, one 256-bit standardized hash, OCI-style text form); `VersionedRef`; `configuration_id` (semantic, seedless) vs `configuration_version_id` (manifest key); `ContentAddress` gains `idp`; `model_call_id`/`tool_call_id` spellings; immutability I1–I5, supersession/`RevocationRecord`/`StaleIndex` enforced in the resolver; sameness ladder L0–L4; reproducibility levels R0–R3 and the bundle as a lock manifest; results rows keyed `(configuration_version_id, run_id)`, aggregated by `configuration_id`.
- **Resource model converged (ADR-0039/0040/0041 with ADR-0043/0045/0046):** closed kernel dimension list (counters vs gauges) adopted as HIR/1 `Budget.dimensions` and as the `eval_budget` equality basis; charges with attribution and `charged_to ∈ {subject, instrument}`; spend derived with six-valued `CostProvenance`, derivation, `confidence ∈ {exact, bounded, estimate, unknown}`, coverage (**OQ-033 answered**); budget nodes with dynamic containment, slice/pool, reserve-before-spend, root-first exhaustion; soft budgets are `HarnessRule` artifacts measured via T-LCD-13, hard budgets fire only at effect boundaries; `MatchSpec` with three modes and engine refusal (T-LCD-14).
- **Measurement backbone (ADR-0042…0047):** spans are projections of the ledger, export is lowering with a telemetry loss report, the ledger is never sampled; M1–M18 measurement points, integer ms and `Money{micro_units}`, inclusive `TokenVector` convention with a stamped `normalizer_ref`; L0–L2 always-on for native runs, sink classification instead of sampling; full `MetricDeclaration` (supersedes three partial forms), six factors + replicate axis, outcome classes, `n/a{reason}`, distributional reporting with pass^k/(c, n); matched-budget protocol with three benefit kinds and C4 gates; nine oracle classes with deterministic-only headline and judge governance. **OQ-037 resolved** (gates vs checklists).
- **Cross-dossier rulings (ADR-0048; CF-101…CF-115):** one identifier per object (compliance chain, structural ids, effect phases, effect domains, authority of lifted content = `unverified`, security event classes, cost fields, `ContentAddress`, `DecisionPoint`); one declaration per seam (risk class as projection; `slots` map; one resolver; `CompensationPlan` as runtime record; L2 budget dimensions); WS-B1 taxonomy amendments consolidated in the ADR-0026 log; scope stage notes (R-2.5.4, R-2.3.3, R-2.1.4/R-2.10.1-2, R-2.8.2, R-2.4.4, R-2.2.5/R-2.5.5).
- **Evidence discipline:** every ratified ADR carries §3.3 (a)–(e); no C0 element rests on a `provisional` result — 2026 preprints (S-061/062/072/073/077/078/083/088/092/095/103–106, S-202–S-211, S-236–S-239) supply mechanism or counter-evidence only and are corroborated by Tier-A theory/standards (Denning, Biba, DLM, Flume, Sagas, Leases, RFC 6962/8785/9562, PROV, SLSA, OCI) and Tier-B source at pinned commits (codex 0735c51, goose fae91d0, software-agent-sdk 3fc7b22, opencode 9f8db11, pi-mono 400d690, harbor 7d5285b, inspect_ai 75f4891, hal-harness 16bb03e, agent-spec 6f0b6ae, omnigent 6a069da, camel f083b6b, fides 669c046, tau-bench 59a200c, agentdojo 089ed46). LCD battery applied to every A3/A4/A5 ADR: all pass on paper (record in `lcd-test-battery.md`). Language-leak audit clean (CF-111); naming audit clean (CF-113).
- **WS-L1:** all questionnaire items answered (OQ-040…046), neither conditional hard gate fires (CF-112), OQ-045 provisional = out-of-process helper binary, OQ-047 spike plan fixed (ADR-0049); the orchestrator runs the ADR-0009 procedure next with `synthesis/l1-inputs.md`.
- **Open and blocking for Phase 2:** OQ-049 (deterministic detectors per artifact kind — T-LCD-13 at Stage 3), OQ-055 (`EffectDomain` extension policy), OQ-063 (renderer ownership D1/C1), OQ-072 (gateway consumes request plan — CF-044), OQ-082 (counterfactual validity without replay), OQ-086 (shell reversibility assessment), OQ-101 (policy table Π), OQ-104 (`role_map`), OQ-113 (R2/R3 margins for I3), OQ-123 (judge calibration); CF-017 stays pre-registered; CF-038/CF-044 pre-registered for Phase 2 synthesis.
- **Registers:** sources next S-242 (75 added, 28 promoted) · open-questions next OQ-130 (82 added; 22 pre-existing rows updated) · conflicts next CF-116 (90 added: 75 workstream-flagged + 15 synthesis-raised; CF-001/006/007/008/011 updated) · ADR next ADR-0050 · scope: 12 items `specified-by-ADR`, stage notes logged (ADR-0048/0049) · risks RK-02/03/05/07/09/11 updated.
- **WS-L1 ratification (2026-09-10, after synthesis):** ADR-0009 procedure executed by a fresh ratification subagent — weights frozen and logged at 14:05:57Z before evidence; C2 gate applied (none excluded; C4/C6 not triggered); spikes replaced by `synthesis/l1-spike-spec.md` (Stage-0 acceptance check; CF-117) with C5/C7/C12 scored `unmeasured-by-spike` and swept with ±1 bands; nine candidates (E1–E4, E5a–E5e) scored with one citation per cell (S-242…S-250 added); sweep: E5a 0.746 win share (< 0.8), E1-kernel family 0.979 → tie-breakers T3 (eliminates single-ecosystem E1) and T4 (late-bind the surface layer). **Decision ADR-0050 (ratified):** kernel + sandbox helper in E1 (compiled memory-safe), laboratory in E2 (dynamically-typed research) over a per-run subprocess + JSON-RPC stdio boundary with generated bindings and exact pinning, surfaces via a generated client over a local transport (E3 default, bound by K2/K4 at Phase 3 — OQ-130); boundary contract (WS-L1 §6.5, I4 per CF-058) adopted as C0/Stage 1 under WS-K4 + WS-L5; binds Stage 0–1, lab boundary first exercised at Stage 3, surface boundary at Stage ≥ 5. ADR-0009 ratified with it. OQ-001 resolved; R-2.12.3 specified; OQ-130/131 opened; CF-116 (accepted tension), CF-117 (resolved). Phase ≥ 2 language-use constraint in ADR-0050 §8. Registers: sources next S-251 · OQ next OQ-132 · CF next CF-118 · ADR next ADR-0051.
- **Next phase (2):** fan out C1–C4 · D1–D5 · E1–E5 · F1 F2 · G1–G3 · H1–H7 · B3 B4 B5 · I3 I4 with the "settled for Phase 2" section of `research/synthesis/phase-1.md` and ADR-0050 §8 (language-use constraint) in every brief; H1/H2/D1/D4 must cite ADR-0033/0034/0035 and may not introduce a second scheme; every Phase 2 ADR states its T-LCD tests and its rung on the ADR-0024 ladder.

### Phase 2 — 2026-09-10
- **Gate: passed.** All 31 Phase 2 workstreams `done`; 94 proposed ADRs dispositioned — 94 ratified (17 amended at synthesis with logged amendment sections), 0 rejected — plus two synthesis-authored ADRs (ADR-0145 reconciliation, ADR-0146 scope). Memo: `research/synthesis/phase-2.md`.
- **CF-006 closed (ADR-0145 §A):** H1 (ADR-0051…0053), H2 (0054…0056), D1 (0072…0074), D4 (0081…0083) and D3 (0080) conform to the WS-L3 scheme; the scheme changed only by logged amendment (kernel stamps `context_label`; check-3 domains; `pin` needs a verified signature; `store` dimension on persistence ceilings; `UnattendedPolicy`/`auto_review` inside the closed basis list). Two residual tensions resolved at synthesis (CF-311 handle-delivered items never join the label; CF-312 one `decider` sum).
- **Effect path composes (ADR-0145 §B):** `resolve → authorize → prepare → commit → execute → capture → observe` over ADR-0030, with H1 `authorize`, H4 `admits ∧ covers` + EP3 egress, H3 broker injection, kernel-minted attribution token, B5 handle, G1/G2/F2/H6 at `observe`; B5 adopted the H4/E5/I4 handle seams verbatim; OQ-045 final = out-of-process helper (no ADR-0050 trigger).
- **Context chain composes (§C):** D1 admission spine; D2 under the kernel gauge cap with `context_exhausted`; D3 candidates with own label + `memory_index`; D4 `validity_policy` supersedes D1's boolean; C3 `prompt_layout` params; C4 `volatile_kinds` at `link`; one `ProviderRequestPlan`.
- **Compilation chain composes (§D) — LCD battery applied:** thirteen-kind `ProfileRuleInventory/1` (C3), closed transform vocabulary with `parse` at C0 (E2; ADR-0022 amended), stateless MCP catalogue + v2/v1 ACP profile (E4; ADR-0021 amended); six C3/E2 ADRs recorded **pass** on paper in `registers/lcd-test-battery.md`; CF-001 validated on paper.
- **CONTROL:** CF-005 resolved by ADR-0103/0106 ("pluggable + envelope"); F2 owns `StopReason`; retries split gateway/F2/C2; H7 approvals are a budgeted dimension with leases and a never-auto set; F1 `Cue.woken` for B3.
- **Verification provenance:** one verdict record (`verification.validator.verdict` extended) for validators, reconcilers and critics; completion gate on `verification.completion.decided`; `false_completion` added to ADR-0047's veto list; no default judge; user simulators are instrument models.
- **Phase 1 ADRs amended (21):** ADR-0016, 0017, 0019, 0020, 0021, 0022, 0026, 0030, 0031, 0034, 0035, 0036, 0037, 0038, 0039, 0040, 0041, 0043, 0044, 0047, 0050 — additive or wording only; index rows marked `ratified, amended (P2)`.
- **Ontology v2:** 513 terms folded (§5b, status `ratified (v2, …)`), 10 vocabulary rulings (§5c), 19 canonical names added to §6.
- **Scope (ADR-0146):** R-2.7.2 split into R-2.7.2a (C0/Should) and R-2.7.2b (C2/Could); stage notes on R-2.2.3, R-2.3.3, R-2.4.2, R-2.4.5, R-2.5.2, R-2.7.1, R-2.8.5, R-2.9.3; SWE-bench Verified demoted to fixture/control; 31 items → `specified-by-ADR`.
- **Evidence discipline:** 94/94 ADRs carry §3.3 (a)–(e) + T-LCD; no C0 ADR rests on a `provisional` claim; language-leak audit clean (CF-317; RK-09); naming audit clean (CF-321); RK-07 exercised and held.
- **Registers:** sources next S-499 (248 added; 13 cross-sidecar duplicates merged; 26 path-level citations folded; 25 promoted S→P) · open-questions next OQ-346 (214 added; 55 existing rows updated) · conflicts next CF-322 (193 workstream-flagged + 11 synthesis-raised; all dispositioned) · ADR next ADR-0147.
- **Open and blocking for Phase 3:** OQ-132/133 (`ResourcePattern` and command grammars), OQ-142 (unattended Π rows), OQ-149 (`SecretRef` in HIR/1), OQ-165 (hooks as guards vs rules), OQ-186 (offload-handle reader), OQ-219/223 (schema dialect; `unknown_domain`), OQ-235/240 (tool-plane events/surface fields), OQ-201 (Π and memory writes), OQ-344 (`applies_to_families`), OQ-170 (signer custody), OQ-246 (approval persistence across the ACP edge); full list in `synthesis/phase-2.md` §5.
- **Next phase (3):** fan out J1–J6 · K1–K4 · L5 with `synthesis/phase-2.md` §6 ("settled for Phase 3": environment handle, event-store views, scorecard, bundle, registry needs, protocol edges, ACP verbs for the Hosting ABI) and ADR-0050 §8 in every brief; CF-017 exposure K2/K4 pre-registered; orchestrator commits at the boundary (RK-10).

