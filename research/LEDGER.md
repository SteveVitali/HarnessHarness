# MetaHarness — Program LEDGER

**Single source of truth for program state** (doc 3 §4, §11.8). A re-invoked session reads this file first and resumes at the first non-`done` workstream in the first non-`passed` phase.

- **Program contract:** `docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md` §11 (Operator's Runbook)
- **Terminal deliverables:** `spec/CANONICAL_SPEC.md`, `spec/READINESS_REPORT.md`
- **Hard stop:** Decompose-Readiness Gate (§7.3). No `decompose-spec` / `orchestrate-build` / `implement-spec`; no framework implementation code.
- **Binding constraints:** language/ecosystem-agnostic until WS-L1 ratifies; `provisional` evidence never load-bearing for C0 (§3.1); every scope change logged as an ADR.
- **Git:** initialized 2026-09-09; commit at every phase boundary.

## Status legend

`todo` → `in-progress` → `dossier-written` (agent returned, not yet synthesized) → `done` (synthesis pass accepted; ADRs dispositioned) · `blocked` · `deferred(ADR-####)`

## Phase gates

| Phase | Contents | Status | Gate criteria (exit) | Commit |
|---|---|---|---|---|
| Preflight | §11.2 steps 1–6 | **passed** | tree exists; sources/ledger/registers seeded; ADR-0001/0002 ratified | preflight commit |
| 0 | WS-L1 (framing only), WS-A1, WS-L7; Ontology v0; Source Registry seeded | todo | A1 + L7 dossiers done; L1 criteria framed (not decided); ontology v0 in `registers/ontology.md` | — |
| 1 | WS-A2 A3 A4 A5 · B1 B2 · L2 L3 L4 · I1 I2 | todo | all 11 done; shared authority/label scheme converged (L3↔A3↔I1); IR vocabulary arbitrated by A3; **WS-L1 language ADR ratified at end of Phase 1** | — |
| 2 | C1–C4 · D1–D5 · E1–E5 · F1 F2 · G1–G3 · H1–H7 · B3 B4 B5 · I3 I4 | todo | all 27 done; L3/H1/D1 authority scheme ratified together (§7.1); conflicts resolved in `conflicts.md` | — |
| 3 | J1–J6 · K1–K4 · L5 | todo | all 11 done; thin observational ABI depth decided (J6); plugin contract (L5) consistent with J2 | — |
| 4 | F3 F4 F5 · I5 I6 I7 I8 · organizational layer (WS-L8, see below) | todo | all 8 done; every C4 decision carries matched-budget conditionality; no C0 decision rests on provisional evidence | — |
| 5 | Synthesis & Canonical Spec | todo | §7.2 skeleton authored; adversarial coherence review clean; `READINESS_REPORT.md` all-green | — |

## Workstream table

Fields per §11.2 step 4: `{id, track, phase, status, blocking-edges, dossier-path, owner-agent}`. `blocking-edges` = workstreams that must be `done` first. `owner-agent` is filled with the subagent label when fanned out.

### Track A — Foundations, Ontology & IR

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-A1 | Positioning & prior-art/competitive audit | A | 0 | todo | — | research/dossiers/WS-A1.md | — | — |
| WS-A2 | Novel ontology: 7-plane model, policy-stack formalism, validity vs compliance, compatibility surface, participant classes | A | 1 | todo | A1, L7 | research/dossiers/WS-A2.md | — | — |
| WS-A3 | Harness IR design: typed entities/edges; behavioral not framework-specific | A | 1 | todo | A1 | research/dossiers/WS-A3.md | — | — |
| WS-A4 | Compilation model: IR → runtime + model profiles + protocol targets | A | 1 | todo | A1 | research/dossiers/WS-A4.md | — | — |
| WS-A5 | Configuration & composition model; code-vs-config boundary | A | 1 | todo | A1 | research/dossiers/WS-A5.md | — | — |

### Track B — Core Runtime & Durability

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-B1 | Event store / run ledger: taxonomy, ID model, materialized views | B | 1 | todo | A1 | research/dossiers/WS-B1.md | — | — |
| WS-B2 | Effect & transaction model: intent/prepared/commit/observed; idempotency; leases; compensation | B | 1 | todo | A1 | research/dossiers/WS-B2.md | — | — |
| WS-B3 | Durable execution & recovery | B | 2 | todo | B1, B2 | research/dossiers/WS-B3.md | — | — |
| WS-B4 | Reversible & speculative execution | B | 2 | todo | B1, B2 | research/dossiers/WS-B4.md | — | — |
| WS-B5 | Execution-environment abstraction | B | 2 | todo | B1, B2, L3 | research/dossiers/WS-B5.md | — | — |

### Track C — Model Plane

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-C1 | Model adapter/gateway | C | 2 | todo | A3, A4 | research/dossiers/WS-C1.md | — | — |
| WS-C2 | Model router | C | 2 | todo | A3, A4, L2 | research/dossiers/WS-C2.md | — | — |
| WS-C3 | Model-profile compiler | C | 2 | todo | A3, A4 | research/dossiers/WS-C3.md | — | — |
| WS-C4 | Caching & token economics | C | 2 | todo | L2 | research/dossiers/WS-C4.md | — | — |

### Track D — Context & Memory

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-D1 | Context builder / policy engine | D | 2 | todo | A3, L3, L2 | research/dossiers/WS-D1.md | — | — |
| WS-D2 | Compaction strategy family | D | 2 | todo | A3, L2 | research/dossiers/WS-D2.md | — | — |
| WS-D3 | Retrieval & memory hierarchy | D | 2 | todo | A3, L3, B1 | research/dossiers/WS-D3.md | — | — |
| WS-D4 | Memory lifecycle semantics | D | 2 | todo | A3, L3, L4 | research/dossiers/WS-D4.md | — | — |
| WS-D5 | Skills & Procedure IR | D | 2 | todo | A3, A4 | research/dossiers/WS-D5.md | — | — |

### Track E — Tools & Action

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-E1 | Tool registry & typed capabilities | E | 2 | todo | A3, L3 | research/dossiers/WS-E1.md | — | — |
| WS-E2 | Tool-interface compiler | E | 2 | todo | A3, A4 | research/dossiers/WS-E2.md | — | — |
| WS-E3 | Tool scaling: discovery, deferred loading | E | 2 | todo | A3 | research/dossiers/WS-E3.md | — | — |
| WS-E4 | Protocol edges: MCP, A2A, ACP | E | 2 | todo | A3, A4 | research/dossiers/WS-E4.md | — | — |
| WS-E5 | Sandboxed tool execution & effect capture | E | 2 | todo | B2, L3 | research/dossiers/WS-E5.md | — | — |

### Track F — Control & Orchestration

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-F1 | Control-strategy family | F | 2 | todo | A3, B1 | research/dossiers/WS-F1.md | — | — |
| WS-F2 | Control envelope | F | 2 | todo | B2, L2 | research/dossiers/WS-F2.md | — | — |
| WS-F3 | Sub-agent orchestrator | F | 4 | todo | B3, B5, H1, L2 | research/dossiers/WS-F3.md | — | — |
| WS-F4 | Value-of-compute scheduler | F | 4 | todo | L2, I2, F3 | research/dossiers/WS-F4.md | — | — |
| WS-F5 | Multi-agent coordination & consistency | F | 4 | todo | B1, B2, F3 | research/dossiers/WS-F5.md | — | — |

### Track G — Verification

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-G1 | Verification fabric | G | 2 | todo | A3, B1 | research/dossiers/WS-G1.md | — | — |
| WS-G2 | Execution-alignment / belief-state reconciliation | G | 2 | todo | B1, L3 | research/dossiers/WS-G2.md | — | — |
| WS-G3 | Independent critics/evaluators | G | 2 | todo | I2 | research/dossiers/WS-G3.md | — | — |

### Track H — Security & Governance

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-H1 | Reference-monitor kernel & capability model | H | 2 | todo | L3, A3 | research/dossiers/WS-H1.md | — | — |
| WS-H2 | Information-flow control & taint labeling | H | 2 | todo | L3 | research/dossiers/WS-H2.md | — | — |
| WS-H3 | Credential mediation & secret isolation | H | 2 | todo | L3 | research/dossiers/WS-H3.md | — | — |
| WS-H4 | Egress/network policy & sandbox boundaries | H | 2 | todo | B5 | research/dossiers/WS-H4.md | — | — |
| WS-H5 | Extension supply-chain trust | H | 2 | todo | L3, L4 | research/dossiers/WS-H5.md | — | — |
| WS-H6 | Deep audit trail & tamper-evident logging | H | 2 | todo | B1, I1 | research/dossiers/WS-H6.md | — | — |
| WS-H7 | HITL/approval & escalation policy | H | 2 | todo | L2 | research/dossiers/WS-H7.md | — | — |

### Track I — Measurement & Evolution

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-I1 | Telemetry/tracing + cost & latency instrumentation | I | 1 | todo | A1 | research/dossiers/WS-I1.md | — | — |
| WS-I2 | Eval framework: factorial design; 8-dim scorecard | I | 1 | todo | A1 | research/dossiers/WS-I2.md | — | — |
| WS-I3 | Reproducible harness bundle format | I | 2 | todo | L4, I2 | research/dossiers/WS-I3.md | — | — |
| WS-I4 | Benchmark/environment integration | I | 2 | todo | I2, B5 | research/dossiers/WS-I4.md | — | — |
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
| WS-L1 | Language/ecosystem selection spike (framed P0, ratified end-P1) | L | 0→1 | todo | (framing: —) (ratify: A3, A4, A5, B1) | research/dossiers/WS-L1.md | — | — |
| WS-L2 | Resource-economics & accounting model | L | 1 | todo | A1 | research/dossiers/WS-L2.md | — | — |
| WS-L3 | Provenance & authority model | L | 1 | todo | A1 | research/dossiers/WS-L3.md | — | — |
| WS-L4 | Versioning, reproducibility & artifact identity | L | 1 | todo | A1 | research/dossiers/WS-L4.md | — | — |
| WS-L5 | Extensibility/plugin architecture & third-party component contracts | L | 3 | todo | A5, J2 | research/dossiers/WS-L5.md | — | — |
| WS-L6 | Packaging, licensing, OSS governance, docs & community | L | 5 | todo | L1 | research/dossiers/WS-L6.md | — | — |
| WS-L7 | Novelty/differentiation thesis & naming | L | 0 | todo | — | research/dossiers/WS-L7.md | — | — |
| WS-L8 | Human-agent organizational layer (§2.12, C4; Phase 4 "organizational layer" — no WS ID in §5, assigned here) | L | 4 | todo | F3, H7, B3 | research/dossiers/WS-L8.md | — | — |

**Count:** 52 workstreams (51 from §5 + WS-L8 assigned to the §6 Phase 4 "organizational layer" item, which §5 names but does not ID). WS-L6 is scheduled in Phase 5 (no §6 phase names it; it depends on WS-L1).

## ADR index

| ADR | Title | Status | Owner WS | Phase |
|---|---|---|---|---|
| ADR-0001 | Positioning: native-primary, hosting-as-participant; two participant classes | **ratified** (pre-ratified by sponsor) | WS-A1, WS-J6 | Preflight |
| ADR-0002 | Evolution / co-evolution fully in-scope, not stubbed | **ratified** (pre-ratified by sponsor) | WS-I5, WS-I8 | Preflight |

*(Workstreams append `ADR-NNNN` rows with `status: proposed`; synthesis passes ratify/amend/reject. Next number: ADR-0003.)*

## Phase progress log

### Preflight — 2026-09-09
- Docs 1–3 read in full by the orchestrator.
- `git init`; initial commit of corpus.
- Created `research/{dossiers,decisions,registers}` and `spec/sections`.
- Seeded `registers/sources.md` (S-001…S-131) from doc 2 Sources + all footnotes + Tier 0–4 syllabus.
- Initialized this LEDGER (52 workstreams), `open-questions.md`, `ontology.md` (v0), `conflicts.md`, `scope.md` (§2 imported with MoSCoW), `risks.md` (§9 imported).
- Wrote ADR-0001 and ADR-0002 as `ratified`.
