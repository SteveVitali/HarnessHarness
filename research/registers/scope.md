# Scope Register — §2 catalogue as tracked requirements

**Seeded:** 2026-09-09 (Preflight, doc 3 §11.2 step 5) from doc 3 §2. **MoSCoW default rule:** `Must` for C0; `Should` for C1; `Could` for C2–C3; `Could*` for C4 (fully specified per ADR-0002 — *not* stubbed — but latest on the build ladder). MoSCoW is continuous and frozen at Phase 5 (doc 3 §8).
**Columns:** `req-id` · item · tier · novelty (⊕ NEW / ↑ EXT) · owner WS · target spec section (§7.2 skeleton) · MoSCoW · status (`open` / `specified` / `deferred(ADR-####)`).

Every item must end Phase 5 as `specified` (interface contract + data model + acceptance criteria + build-stage) or `deferred(ADR)` — never dropped (§1.4, §7.3).

## 2.1 Foundations (C0)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.1.1 | Ontology / conceptual systematization (7-plane + policy-stack formalism + validity vs compliance + compatibility surface + two participant classes) | C0 | — | WS-A2 (A1, L7) | 2 | Must | open |
| R-2.1.2 | Harness IR — typed behavioral entities + edges | C0 | ⊕ NEW | WS-A3 | 3 | Must | open |
| R-2.1.3 | Compilation model — IR → runtime + model profiles + protocol targets | C0 | ⊕ NEW | WS-A4 | 3 | Must | open |
| R-2.1.4 | Configuration & composition model; code-vs-config boundary | C0 | ↑ EXT | WS-A5 | 3, 6 | Must | open |
| R-2.1.5 | Provenance & authority model | C0 | ⊕ NEW | WS-L3 | 8 | Must | open |
| R-2.1.6 | Resource-economics & accounting model | C0 | ↑ EXT | WS-L2 | 8 | Must | open |

## 2.2 Core runtime & durability (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.2.1 | Event store / run ledger | C0 | — | WS-B1 | 5 | Must | open |
| R-2.2.2 | Effect & transaction model | C0 | ⊕ NEW | WS-B2 | 5 | Must | open |
| R-2.2.3 | Durable execution & recovery | C1 | ↑ EXT | WS-B3 | 5 | Should | open |
| R-2.2.4 | Reversible & speculative execution | C2 | ⊕ NEW | WS-B4 | 5 | Could | open |
| R-2.2.5 | Execution-environment abstraction | C0 | ↑ EXT | WS-B5 | 5 | Must | open |

## 2.3 Model plane (C0/C1)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.3.1 | Model adapter / gateway | C0 | ↑ EXT | WS-C1 | 5 | Must | open |
| R-2.3.2 | Model router | C1 | ↑ EXT | WS-C2 | 5 | Should | open |
| R-2.3.3 | Model-profile compiler | C1 | ⊕ NEW | WS-C3 | 5 | Should | open |
| R-2.3.4 | Caching & token economics | C1 | — | WS-C4 | 5 | Should | open |

## 2.4 Context & memory plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.4.1 | Context builder / policy engine | C0 | ↑ EXT | WS-D1 | 5 | Must | open |
| R-2.4.2 | Compaction strategies (pluggable family) | C1 | ↑ EXT | WS-D2 | 5, 6 | Should | open |
| R-2.4.3 | Retrieval & memory hierarchy | C1 | — | WS-D3 | 5 | Should | open |
| R-2.4.4 | Memory lifecycle semantics | C2 | ⊕ NEW | WS-D4 | 5 | Could | open |
| R-2.4.5 | Skills & Procedure IR | C2 | ⊕ NEW | WS-D5 | 3, 5 | Could | open |

## 2.5 Tools & action plane (C0/C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.5.1 | Tool registry & typed capabilities | C0 | ↑ EXT | WS-E1 | 5 | Must | open |
| R-2.5.2 | Tool-interface compiler | C2 | ⊕ NEW | WS-E2 | 5 | Could | open |
| R-2.5.3 | Tool scaling (discovery, deferred loading) | C1 | — | WS-E3 | 5 | Should | open |
| R-2.5.4 | Protocol edges — MCP (client+server), A2A, ACP | C1 | ↑ EXT | WS-E4 | 5, 7 | Should | open |
| R-2.5.5 | Sandboxed tool execution & effect capture | C0 | — | WS-E5 | 5 | Must | open |

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
| R-2.8.1 | Reference-monitor security kernel & capability model | C0 | ⊕ NEW | WS-H1 | 5 | Must | open |
| R-2.8.2 | Information-flow control & taint labeling | C2 | ⊕ NEW | WS-H2 | 5 | Could | open |
| R-2.8.3 | Credential mediation & secret isolation | C0 | ⊕ NEW | WS-H3 | 5 | Must | open |
| R-2.8.4 | Egress / network policy & sandbox boundaries | C0 | ⊕ NEW | WS-H4 | 5 | Must | open |
| R-2.8.5 | Extension supply-chain trust | C1 | ⊕ NEW | WS-H5 | 5, 8 | Should | open |
| R-2.8.6 | Deep audit trail & tamper-evident logging | C0 | ↑ EXT | WS-H6 | 5 | Must | open |
| R-2.8.7 | HITL / approval & escalation policy | C1 | ⊕ NEW | WS-H7 | 5 | Should | open |

## 2.9 Measurement & evolution plane (C0/C1/C4)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.9.1 | Telemetry, tracing & cost/latency instrumentation | C0 | ↑ EXT | WS-I1 | 5 | Must | open |
| R-2.9.2 | Eval framework (factorial; 8-dim scorecard; distributions; process metrics) | C0 | ↑ EXT | WS-I2 | 5, 10 | Must | open |
| R-2.9.3 | Reproducible harness bundle | C1 | ⊕ NEW | WS-I3 | 5, 10 | Should | open |
| R-2.9.4 | Benchmark/environment integration (coding/terminal first) | C1 | — | WS-I4 | 10 | Should | open |
| R-2.9.5 | Evolution service (governed pipeline) | C4 | ↑ EXT | WS-I5 | 5 | Could* (ADR-0002: fully specified) | open |
| R-2.9.6 | Assumption-debt manager | C4 | ⊕ NEW | WS-I6 | 5 | Could* | open |
| R-2.9.7 | Causal attribution & counterfactual execution | C4 | ⊕ NEW | WS-I7 | 5 | Could* | open |
| R-2.9.8 | Model-harness co-evolution & consolidation | C4 | ⊕ NEW | WS-I8 | 5 | Could* (ADR-0002) | open |

## 2.10 Meta-harness laboratory (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.10.1 | Harness assembly & declarative definition | C1 | ↑ EXT | WS-J1 | 6 | Should | open |
| R-2.10.2 | Component-variation registry | C1 | ↑ EXT | WS-J2 | 6 | Should | open |
| R-2.10.3 | Experiment & sweep engine | C1 | ⊕ NEW | WS-J3 | 6 | Should | open |
| R-2.10.4 | Comparison & analysis engine (both participant classes) | C2 | ⊕ NEW | WS-J4 | 6 | Could | open |
| R-2.10.5 | Results store, experiment ledger & leaderboard | C1 | ⊕ NEW | WS-J5 | 6 | Should | open |
| R-2.10.6 | External-harness hosting / thin observational ABI | C2 | ⊕ NEW | WS-J6 | 6 | Could (first-class, secondary per ADR-0001) | open |

## 2.11 Surfaces (C1/C2)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.11.1 | CLI | C1 | — | WS-K1 | 7 | Should | open |
| R-2.11.2 | Web-based local dev-tooling | C2 | ↑ EXT | WS-K2 | 7 | Could | open |
| R-2.11.3 | MCP server | C2 | ↑ EXT | WS-K3 | 7 | Could | open |
| R-2.11.4 | SDK / embedding API | C1 | ⊕ NEW | WS-K4 | 7 | Should | open |

## 2.12 Cross-cutting & program concerns (C0/L)

| req-id | item | tier | novelty | owner WS | spec § | MoSCoW | status |
|---|---|---|---|---|---|---|---|
| R-2.12.1 | Versioning, reproducibility & artifact identity | C0 | ↑ EXT | WS-L4 | 8 | Must | open |
| R-2.12.2 | Extensibility / plugin architecture & third-party component contracts | C0 | ↑ EXT | WS-L5 | 8 | Must | open |
| R-2.12.3 | Language/ecosystem selection (deferred, criteria-driven; outside C-tiers) | — | — | WS-L1 | 11 / ADR | Must (as a decision) | open |
| R-2.12.4 | Packaging, licensing, OSS governance, docs & community | — | — | WS-L6 | 11 | Should | open |
| R-2.12.5 | Novelty/differentiation thesis & naming | — | — | WS-L7 | 1 | Must | open |
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
