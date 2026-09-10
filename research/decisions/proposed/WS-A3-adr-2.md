# ADR-NNNN — HIR/1 entity and edge catalogue with provenance and version records

**Status:** proposed
**Owner workstream(s):** WS-A3 (consumers: every Phase 1–3 workstream; vocabulary arbitration under ADR-0008) · **Phase:** 1 · **Related:** ADR-0001, ADR-0002 (governance constraints iii/iv), ADR-0004, ADR-0006 (novelty claim a), ADR-0007 (T-LCD-05/-13/-14/-15 upstream fields), ADR-0008; CF-006, CF-008, CF-011, CF-WS-A3-02/-03/-04/-05; OQ-006, OQ-014, OQ-020, OQ-023; R-2.1.2, R-2.1.5, R-2.2.2

## Context

Ontology v0.1 §4b lists the doc 2 §11 entities and edges as "not yet typed; WS-A3 owns". Phase 0 made the entity set load-bearing: novelty claim (a) holds only if the IR spans all seven planes with `Permission, Effect, Validator, Budget, Memory` (with validity/authority) and `HarnessRule` (with assumption-debt fields) alongside `Goal, Observation, ContextItem, Procedure, ToolCapability, Artifact, AgentProcess`, every entity versioned with provenance (phase-0.md §5.2; CF-011 re-check at Phase 1). The audit found that no existing system carries `Permission` with attenuating delegation, `Budget` with monotone containment, `Memory` with validity, or a rule with an assumption-debt record (dossier §5 negative results), while OpenHands supplies the event/source/parent/supersession shapes, MCP the tool shape, and Agent Spec the versioned-document shape.

## Options considered

1. **Adopt Open Agent Spec's entity set** (agent/tool/flow/LLM-config) and extend by metadata. *Rejected:* permission, effect, validator, budget, memory-validity and provenance would live in untyped `metadata` — exactly the LCD escape hatch; CF-011 would revert claim (a) to "renamed".
2. **Adopt the doc 2 §11 list verbatim** (13 entities, 6 edges) with free-form fields. *Rejected:* under-specified; `Effect` conflates a type and a runtime record; no edit-provenance edge; no leaf types; invariants unstated.
3. **Doc 2 list refined: 13 entities + 2 leaves + 7 edges, with fixed semantic/surface fields, invariants, provenance and version records, and `Effect` split into `EffectClass` (type) and `Effect` (record) (chosen).**

## Decision

Adopt the catalogue in WS-A3 dossier §6.2 as **HIR/1**:

- **Entities (13):** `Goal, Observation, ContextItem, Memory, Procedure, ToolCapability, Permission, Effect (record) with EffectClass (closed sum), Artifact, Validator, AgentProcess {native | hosted}, Budget, HarnessRule` — each with the semantic fields, profile-owned surface fields and invariants listed there. **Leaves (2):** `Text`, `CompiledPayload`. **Edges (7):** `depends-on, supersedes, authorizes, produced-by, validates, delegated-to, derived-from` (the seventh is new: edit provenance `{hypothesis, trajectories[], candidate_id?}`).
- **Provenance record on every node and edge:** `{origin ∈ human | model | tool | evolution | import | migration (each with typed refs), authority: AuthorityClass, created_at, attestation?}`. **Version record:** `{version_id, semantic_id, dialect, supersedes?, sealed}`.
- **Kernel invariants enforced by `validate`:** V-EFF (Procedure effects covered by in-scope `authorizes`); delegation attenuates (`delegated-to` carries Permission ⊆ parent and Budget ≤ parent); Budget monotone containment; `Permission.issuer.authority ≠ model_claim`; Observation authority never exceeds its source and `model_claim` never satisfies a Validator alone; Memory revocation is a new version + `supersedes`, never deletion; every Goal and AgentProcess references a Budget (upstream of T-LCD-14); `HarnessRule.conditioned_on ≠ null ⇒` complete assumption-debt record (T-LCD-05); `AgentProcess.hosted` has no component sub-entities (T-LCD-06/-15); `supersedes`/`delegated-to`/`depends-on` acyclic within a sealed definition; every `Loop` step bounded by a Budget; every `Opaque` step/`CompiledPayload` has a declared interface.
- **Vocabulary arbitration (ADR-0008 authority):** `semantic identity` = identity over the canonical semantic projection (not equivalence); `Effect` → `EffectClass` + `Effect` record; bare "capability" is banned in spec prose — use `ToolCapability`, `Permission`, `capability declaration/vector`, and `authority handle` (WS-H1 runtime object); `Observation` (IR payload type) vs WS-B1's event envelope; `conditioned rule` = `HarnessRule` with `conditioned_on`.
- **Ownership boundaries:** WS-B2 owns transaction/idempotency semantics of the `Effect` record (shape fixed here); WS-L3/H1/H2/D1 fix `AuthorityClass` *values* (shape fixed here, CF-006); WS-D5 may extend `Procedure` only via `ext` or an HIR/2 dialect; WS-L2 owns the accounting the `Budget.accounting` ref points to.

## Evidence (doc 3 §3.3 synthesis contract — all five mandatory)

- (a) Mechanism evidence: provenance/source/parent and supersession on immutable records — OpenHands `Event{source, parent_id}`, `Condensation{forgotten_event_ids}` (S-110); tool effects as declarations vs hints — MCP `ToolAnnotations` (S-153), OpenHands `DeclaredResources` tri-state (S-110); permissions enforced outside the model — CaMeL (S-099), Claude SDK `PermissionUpdate` (S-109 via WS-A1); versioned documents with references — Agent Spec (S-138/S-139); procedures with typed relations and gated edits — Procedural Graphs (S-095, `provisional (mech)`), AFlow `modification` (S-025); memory validity/revocation failure modes — S-077/S-106 (`provisional (mech)`), corroborated by doc 2 §4/§5.1 (Tier B synthesis) and ADR-0007's T-LCD-13; assumption debt observed in production — S-047, S-049 (Tier B, via WS-L7). Model-claim non-authority — Harness-Bench execution-alignment failure class (S-062, `provisional (mech)`) plus doc 2 §5.3 (Tier B).
- (b) Source-code precedent: `openhands-sdk/openhands/sdk/event/{base.py,types.py,condenser.py}`, `tool/tool.py` L215–282, `profiles/agent_profile.py` L77–226 (native/acp variants), `security/risk.py` (prediction ≠ declaration); `modelcontextprotocol/schema/2026-07-28/schema.ts` L1900–2016; `pyagentspec/src/pyagentspec/{component.py,tools/tool.py}`; `stanfordnlp/dspy/dspy/predict/predict.py` L71–90 (typed learned state); `FoundationAgents/AFlow/scripts/optimizer.py` L26–30; `ShengranHu/ADAS/_arc/search.py` L295–330. **`no-precedent / novel`** for: `Permission` with attenuating delegation, `Budget` monotone containment, `Memory` validity interval, `HarnessRule` assumption-debt field, `derived-from` edge, and the seven-plane set as a whole (consistent with WS-A1 §6.2 item 1 and CF-011).
- (c) Disconfirming evidence considered: provenance on every node is verbose and Agent Spec omits it (S-139) — cost is bytes, benefit is the measurement/evolution plane and the CF-006 scheme; the value of white-box decomposition may be small if benefit localizes (S-092, S-083, `provisional`) — this argues for a minimal C0 kernel, which this catalogue is (no plane gets more than one entity beyond doc 2's list); `Memory` validity rests on `provisional` mechanism papers — mitigated because the field shape (interval + condition + supersedes) is standard temporal-record practice and costs nothing if unused. None overturns.
- (d) Conditionality: model- and task-independent; `EffectClass` initial list assumes coding/computer-use environments and will be extended by dialect for other domains; `Validator.judge` assumes a profile per judge model.
- (e) Build-stage assignment: **C0 / Stage 1** (schema + `validate`); Memory/Procedure entities are populated at Stage 2; Validator/Budget enforcement at Stages 1–3; `AgentProcess.hosted` populated at C2/Stage 4.

**T-LCD statement.** Satisfies T-LCD-01/-02/-06/-10/-12 (via ADR-1's discipline applied to every kind) and supplies the schema fields for T-LCD-05 (assumption-debt), T-LCD-13 (`ContextItem.delivery_id`), T-LCD-14 (Budget on every Goal/AgentProcess) and T-LCD-15 (hosted variant has no components). Does not satisfy T-LCD-03/-04 alone (executable; Stage 3 with WS-A4/I2).

## Consequences

- Ontology v1 (WS-A2) folds §6.2 as the typed §4b and the arbitrations above; the `surface vs semantic identity` row is reworded.
- CF-011 re-check at Phase 1 synthesis: the seven-plane set with provenance is delivered *on paper*; the claim stays conditional on T-LCD-03/-04 at Stage 3.
- WS-B1 must carry `Observation` payloads by reference or value with the IR's authority field intact; WS-H1 must consume `Permission` and `EffectClass` as declared and produce enforcement decisions as Observations with `source = validator`.
- Any later entity proposal is an HIR/2 dialect change with a migration, not an in-place addition.

## Reversibility

**Medium.** Adding entities/edges is a dialect bump (cheap); removing `Permission`, `Budget`, `Memory` validity or `HarnessRule` assumption-debt fields would collapse novelty claim (a) and unpick T-LCD-05/-14/-15 — expensive in program terms though trivial in bytes.
