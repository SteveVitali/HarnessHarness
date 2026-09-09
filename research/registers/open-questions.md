# Open Questions Register

**Seeded:** 2026-09-09 (Preflight). Columns: `id` · question · raised-by · resolver WS · phase-due · blocking? · status (`open` / `resolved(ADR-####|dossier)` / `deferred(ADR-####)`).
**Rule (doc 3 §8):** any question reaching Phase 5 unresolved becomes an explicit deferral ADR — never silently dropped.

## Strategic questions (doc 3 §8 decision gates)

| id | question | raised-by | resolver | due | blocking? | status |
|---|---|---|---|---|---|---|
| OQ-001 | Language/ecosystem selection — criteria and decision | doc 3 §8 | WS-L1 | end Phase 1 | yes (Phase 2 prototyping; Phase 5) | open — framed in Phase 0 |
| OQ-002 | IR expressiveness ceiling: how much behavior is typed vs left to compiled code? | doc 3 §8 | WS-A3 | Phase 1 | yes (all planes) | open |
| OQ-003 | Event-sourcing scope: what is the authoritative log vs materialized view? | doc 3 §8 | WS-B1 | Phase 1 | yes (B3/B4/H6/J5) | open |
| OQ-004 | Depth of the thin observational ABI (canonical name: Hosting ABI): what must hosted participants expose? | doc 3 §8; ADR-0001 | WS-J6 | Phase 3 | yes (J3/J4) | open — reframed by ADR-0005: "minimum observable event set satisfiable by each of the three hosting mechanisms + per-participant capability declaration"; grey-box interposition option in OQ-038; ADR-0007 T-LCD-06/-07 bound the answer |
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

## Workstream additions (append-only; next id **OQ-048** — see end of file)

| id | question | raised-by | resolver | due | blocking? | status |
|---|---|---|---|---|---|---|
| OQ-030 | Interchange format: adopt ATIF (Harbor RFC 0001, S-132) as the hosted-participant trajectory export/import target, or define a documented superset with a lossless mapping for hosted rows? What compaction/subagent markers must it carry? | WS-A1 | WS-J6 (with WS-B1, WS-I3) | Phase 3 | yes (J5 import rows) | open (ADR-0005 fixes the obligation: "ATIF-compatible or documented mapping"; format choice is J6's) |
| OQ-031 | Should the Harness IR be exportable to Open Agent Spec (S-138/S-139) and importable from it as an interop target, so Agent Spec's six runtime adapters become compile targets? What is lost (permissions, effects, validators, provenance)? | WS-A1 | WS-A3 / WS-A4 | Phase 1 | no | open |
| OQ-032 | Is "an IR-native harness can run as a participant inside Harbor/Inspect/Omnigent-style planes via an ACP-style session interface" a C2 acceptance criterion (external validity of the reference runtime)? | WS-A1 | WS-J6, WS-K4 | Phase 3 | no | resolved(ADR-0005): yes — minimum obligation, C2 / Stage 4; J6/K4 specify the exact surface |
| OQ-033 | Cost attribution rules per hosting mechanism (measured model I/O vs participant-reported usage vs native-log reconstruction): how is confidence recorded and how do Pareto analyses treat mixed-confidence rows? | WS-A1 | WS-L2, WS-J4 | Phase 1–3 | no | open |
| OQ-034 | Does a DRIFT conformance verdict exclude a participant version from comparisons or merely annotate it? Who re-probes and when (assumption-debt-style expiry for declarations)? | WS-A1 | WS-J6, WS-I6 | Phase 3 | no | open |
| OQ-035 | Product name "MetaHarness" collides with ruvnet/metaharness (S-149), SuperagenticAI/metaharness (S-150), the Stanford Meta-Harness paper/repo (S-080/S-151) and Omnigent's self-description (S-118). Retain unqualified, retain with a public qualifier ("MetaHarness — harness laboratory & reference runtime"), or rename? | WS-L7 (WS-A1 CF-013) | sponsor via WS-L6 | Phase 5 | no | open (ADR-0008 retains the name and recommends the qualifier; sponsor decides) |
| OQ-036 | Exact definition of the **opacity ratio** (T-LCD-02): token-based vs entity-based; whether tool descriptions count as typed surfaces or text leaves. | WS-L7 | WS-A3 with WS-I2 | Phase 1 | no | open |
| OQ-037 | Which T-LCD tests become automated gates in the build (compiler/engine preconditions) vs review checklists in the readiness report? | WS-L7 | Phase 1 synthesis; WS-I2 | Phase 1 / Phase 5 | no | open (ADR-0007 fixes stage placement; gate-vs-checklist split is Phase 1's) |
| OQ-038 | Should the Hosting ABI offer an optional **model-call interposition** capability (Inspect-style proxying of a hosted participant's model calls) as a declared "grey-box" observability level — giving per-call cost/latency and model-override without pretending to decompose the participant? Feeds OQ-004. | WS-L7 | WS-J6 with WS-C1 | Phase 3 | no | open (ADR-0005 already names model-boundary interception as one of three hosting mechanisms; this asks whether it is *also* an optional capability of session-ABI participants) |
| OQ-039 | Which reference harness anchors T-LCD-03 (behavioural round-trip): a mini-SWE-agent-class Stage-0 baseline only, or additionally a richer open harness (OpenHands SDK / Pi) once the IR matures? | WS-L7 | WS-A3, WS-I4 | Phase 1 (baseline) / Phase 3 (richer) | no | open |
| OQ-040 | Q-L1-01/02/04: Does the IR require closed sum types with exhaustiveness checking, statically checked effect/capability typing, and structural (typed) diffs with source round-trip? (C6 hard-gate switch for the language decision) | WS-L1 | WS-A3 | Phase 1 | yes (language ratification, ADR-0009) | open — **WS-A3 must answer in its dossier** |
| OQ-041 | Q-L1-03: Must IR documents and events have a canonical deterministic byte encoding (canonical JSON / deterministic CBOR / protobuf / hash-over-canonical-form) for content addressing (L4) and tamper evidence (H6)? | WS-L1 | WS-A3 + WS-B1 (consult L4, H6) | Phase 1 | yes | open — **WS-A3/WS-B1 must answer** |
| OQ-042 | Q-L1-05/06: Is the event log authoritative with derived views (OQ-003); what write-path consistency (single writer, leases, fencing) and storage class are mandated at Stage 0–1; does durability need deterministic control-loop replay or event reconstruction only? | WS-L1 | WS-B1 (B3 preview) | Phase 1 | yes | open — **WS-B1 must answer** |
| OQ-043 | Q-L1-07/08: Are compiled control structures runtime-interpreted data or emitted host-language code; must the compiler emit protocol-target artifacts (MCP schemas, A2A AgentCards, ACP session config)? | WS-L1 | WS-A4 | Phase 1 | yes | open — **WS-A4 must answer** |
| OQ-044 | Q-L1-09/10: Is a harness definition a data document loadable by any runtime or does composition require host-language code; must component variants load in-process or may they be out-of-process participants? | WS-L1 | WS-A5 (consult L5) | Phase 1 | yes | open — **WS-A5 must answer** |
| OQ-045 | Q-L1-12: Does the sandbox helper (seccomp/Landlock/Seatbelt/job-object driver) live in the kernel process or as a separate helper binary behind a narrow protocol? (C4 hard-gate switch; provisional answer needed at end of Phase 1) | WS-L1 | WS-B5 / WS-E5 (consult H4); Phase 1 synthesis gives the provisional answer | Phase 1 (provisional) / Phase 2 (final) | yes (provisional) | open |
| OQ-046 | Q-L1-11/13/14: repo-local ecosystem tally of reference systems (WS-A1 — not yet answered; WS-A1's audit lists the repos, Phase 1 synthesis can tally from S-166 + §3 tables); per-sample vs per-run external scorer calls (WS-I2); in-process vs generated-client SDK boundary (WS-K4 preview) | WS-L1 | WS-A1, WS-I2, WS-K4 | Phase 1 (K4 preview) | no | open |
| OQ-047 | Should the two language-ratification spikes (kernel slice; boundary slice) be run by a fresh subagent per candidate under one spec, and who reviews for ecosystem bias? | WS-L1 | Phase 1 synthesis / orchestrator | end Phase 1 | no | open |

*Next id: **OQ-048**.*
