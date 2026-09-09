# Open Questions Register

**Seeded:** 2026-09-09 (Preflight). Columns: `id` · question · raised-by · resolver WS · phase-due · blocking? · status (`open` / `resolved(ADR-####|dossier)` / `deferred(ADR-####)`).
**Rule (doc 3 §8):** any question reaching Phase 5 unresolved becomes an explicit deferral ADR — never silently dropped.

## Strategic questions (doc 3 §8 decision gates)

| id | question | raised-by | resolver | due | blocking? | status |
|---|---|---|---|---|---|---|
| OQ-001 | Language/ecosystem selection — criteria and decision | doc 3 §8 | WS-L1 | end Phase 1 | yes (Phase 2 prototyping; Phase 5) | open — framed in Phase 0 |
| OQ-002 | IR expressiveness ceiling: how much behavior is typed vs left to compiled code? | doc 3 §8 | WS-A3 | Phase 1 | yes (all planes) | open |
| OQ-003 | Event-sourcing scope: what is the authoritative log vs materialized view? | doc 3 §8 | WS-B1 | Phase 1 | yes (B3/B4/H6/J5) | open |
| OQ-004 | Depth of the thin observational ABI: what must black-box participants expose? | doc 3 §8; ADR-0001 | WS-J6 | Phase 3 | yes (J3/J4) | open |
| OQ-005 | MoSCoW prioritization of §2 within the full-vision spec | doc 3 §8 | scope.md / orchestrator | continuous; frozen Phase 5 | no | open |

## Questions inherited from doc 2 §10 (research problems the spec must answer or explicitly defer)

| id | question | raised-by | resolver | due | blocking? | status |
|---|---|---|---|---|---|---|
| OQ-006 | Harness representation: right IR for prompts, context policies, tool interfaces, state machines, validators, permissions, memory, subagent topology | doc 2 §10 | WS-A3 | Phase 1 | yes | open |
| OQ-007 | Causal attribution: can trajectory interventions support counterfactual attribution rather than post-hoc stories? Where does nondeterminism invalidate it? | doc 2 §10 | WS-I7 (B4) | Phase 4 | no | open |
| OQ-008 | Safe self-modification: how to propose/test changes without reward hacking, disabling safeguards, overfitting a local suite, or widening permissions | doc 2 §10 | WS-I5 (H1) | Phase 4 | no | open |
| OQ-009 | Assumption debt: how should model-specific workarounds carry evidence, expiry, and automated retirement tests? | doc 2 §10 | WS-I6 | Phase 4 | no | open |
| OQ-010 | Memory semantics for conflicting, stale, adversarial, low-confidence experience | doc 2 §10 | WS-D4 | Phase 2 | no | open |
| OQ-011 | Execution alignment: detecting narrative-vs-state divergence before false completion | doc 2 §10 | WS-G2 | Phase 2 | no | open |
| OQ-012 | Value-of-compute orchestration: when is a subagent / alt model / evaluator / longer search worth its marginal cost? | doc 2 §10 | WS-F4 | Phase 4 | no | open |
| OQ-013 | Multi-agent consistency: coordinating writes, merging beliefs, reserving resources, resolving conflicts without one giant context | doc 2 §10 | WS-F5 | Phase 4 | no | open |
| OQ-014 | Security capability systems: permissions as explicit transferable capabilities with taint/provenance and least-privilege delegation | doc 2 §10 | WS-H1, WS-H2 | Phase 2 | yes (C0) | open |
| OQ-015 | Durable semantics: idempotency, checkpoint, transaction, exactly-/at-least-once for real-world agent actions | doc 2 §10 | WS-B2, WS-B3 | Phase 1–2 | yes (C0) | open |
| OQ-016 | Benchmark science: measuring effect size, interaction effects, long-tail failure, transfer without leaking benchmark policy into the harness | doc 2 §10 | WS-I2, WS-J4 | Phase 1/3 | yes (C0 eval backbone) | open |
| OQ-017 | Human-agent organizations: escalation, review, accountability, observability at ticket/goal level | doc 2 §10 | WS-L8, WS-H7 | Phase 4 | no | open |
| OQ-018 | Validity vs compliance: can we predict whether a model will activate and follow a correct artifact; can compliance be measured independently? | doc 2 §10 | WS-A2, WS-I2 | Phase 1 | no | open |
| OQ-019 | Compatibility surfaces: learn a response surface over model × task × context budget × tool ABI × environment and compile model-specific profiles from a common representation? | doc 2 §10 | WS-A2, WS-C3, WS-J4 | Phase 1–3 | no | open |
| OQ-020 | Artifact lifecycle & revocation: inheritance of invalidation across downstream artifacts; what stays immutable for audit | doc 2 §10 | WS-D4, WS-L4, WS-H6 | Phase 2 | no | open |
| OQ-021 | Reference-monitor minimality: smallest deterministic TCB enforcing authority/provenance/IFC/effect policy; do capability + IFC designs scale to messy production tools? | doc 2 §10 | WS-H1, WS-H2 | Phase 2 | yes (C0) | open |
| OQ-022 | Safe extension supply chains: pin/sign/sandbox/taint/update/audit skills, hooks, plugins, MCP servers, fetched instructions — including when the agent installs extensions | doc 2 §10 | WS-H5 | Phase 2 | no | open |
| OQ-023 | Procedural IR: representation between prose memory and arbitrary code supporting typed pre/post, composition, retrieval, validation, migration, repair | doc 2 §10 | WS-D5 | Phase 2 | no | open |
| OQ-024 | Counterfactual execution over event-sourced traces without full environment reruns | doc 2 §10 | WS-B4, WS-I7 | Phase 2/4 | no | open |
| OQ-025 | Optimization economics: which harness component deserves the next unit of evaluation compute? | doc 2 §10 | WS-F4, WS-I5 | Phase 4 | no | open |
| OQ-026 | Consolidation between harness and weights: what stays explicit/reversible vs distilled; avoiding compatibility breakage | doc 2 §10 | WS-I8 | Phase 4 | no | open |
| OQ-027 | Organizational semantics for event/ticket/webhook-triggered fleets: accountability, escalation, ownership, audit | doc 2 §10 | WS-L8 | Phase 4 | no | open |

## Program questions raised at Preflight

| id | question | raised-by | resolver | due | blocking? | status |
|---|---|---|---|---|---|---|
| OQ-028 | Where does WS-L6 (packaging/licensing/OSS governance) sit in the phase DAG? §6 does not place it. Preflight placed it in Phase 5 after WS-L1. | orchestrator | orchestrator | Phase 5 | no | resolved (LEDGER placement; revisit if L1 lands earlier) |
| OQ-029 | Should the §6 Phase 4 "organizational layer" be its own workstream? Preflight assigned WS-L8. | orchestrator | orchestrator | Phase 4 | no | resolved (WS-L8 in LEDGER) |

## Workstream additions (append-only; next id **OQ-030**)

| id | question | raised-by | resolver | due | blocking? | status |
|---|---|---|---|---|---|---|
