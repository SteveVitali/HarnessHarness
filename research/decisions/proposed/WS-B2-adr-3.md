# ADR-NNNN — Compensation as an audited, idempotent, best-effort saga over effects with an `abandoned` terminal state

**Status:** proposed
**Owner workstream(s):** WS-B2 (consumers: WS-B3, WS-B4, WS-H1, WS-H6, WS-G2, WS-A3, WS-F1) · **Phase:** 1 · **Related:** ADR-0002 (every accepted change reversible; governance), ADR-0007 (T-LCD-12), ADR-0011; WS-B2-adr-1, WS-B2-adr-2; WS-A3-adr-1; CF-WS-B2-02; OQ-015, OQ-WS-B2-03, OQ-WS-B2-04, OQ-WS-B2-07; R-2.2.2, R-2.2.4

## Context

Doc 2 §5.5: "compensating actions handle operations that cannot be rolled back"; §12 direction 5 lists compensation among the durable-runtime primitives "where side effects can be irreversible". WS-B2-adr-2 defines a `compensable` reversibility class whose approval default and `unknown`-handling depend on a *registered* compensator existing and being authorisable. The classic result (Sagas, S-WS-B2-01) is that compensation restores "an acceptable approximation", not the prior state, that compensators can fail, and that the coordinator must record its progress reliably so a crashed saga can be resumed without re-applying undo steps. Shepherd (S-078) concedes that reverting a trace does not undo external effects; Revisable-by-Design (S-WS-B2-09) proves conflicting irreversible actions are unrecoverable by any algorithm. Two 2025–2026 agent systems (SagaLLM S-WS-B2-06, RAC S-WS-B2-07) apply sagas to agents, both `provisional`. No audited production harness implements compensation; Codex, OpenHands and Symphony rely on git/branch/workspace discard, i.e. the `reversible` class only.

## Options considered

1. **No compensation primitive; rely on `reversible` (snapshots/git) and human cleanup for everything else.** *Rejected:* leaves the `compensable` class undefined, so every PR/ticket/reservation effect becomes `irreversible` and requires per-effect approval — approval volume without safety benefit.
2. **Model-driven compensation** (the model is told to undo on failure). *Rejected as the mechanism:* undo becomes another nondeterministic action with no idempotence or ordering guarantee; RAC's `provisional` result that log-based recovery beats LLM-driven recovery points the same way, though it is not load-bearing.
3. **Transactional (ACID) semantics across tools.** *Rejected:* no external target participates in a harness transaction; the only general mechanism is semantic compensation.
4. **Registered, idempotent, kernel-authorisable compensators executed as effects by a runtime saga coordinator, with `abandoned` as an explicit, audited terminal state (chosen).**

## Decision

1. **Schema (C0 / Stage 1).** `CompensationPlan{plan_id, effect_id, compensator: EffectIntentTemplate, preconditions[], idempotent: true, expires_at?, owner, provenance}` is registered at `prepare` for every `compensable` effect (mandatory — `CompensatorMissing` otherwise) and stored by reference on the `Effect` record (`compensation_plan_id`); whether it is an HIR entity or a field group is OQ-WS-B2-03 (WS-A3).
2. **Authorisation precondition.** A `compensable` effect is not authorised unless its compensator would be authorised *now* under the same grant; the compensator's risk class may not be more dangerous than the original's.
3. **Execution (C1 / Stage 2).** `compensate_run(run_id | branch_id)` runs compensators in reverse commit order for effects in `observed(applied)` marked for undo (run abort, branch discard from WS-B4, approval revocation, explicit request). Each compensator is a new effect with a `compensates` edge and its own full lifecycle (ADR-1). Compensators are idempotent, so re-running a saga interrupted mid-way (crash, lease expiry) applies no additional external effect.
4. **Failure is explicit.** A compensator that fails, is refused, or whose preconditions no longer hold yields `action.effect.abandoned{reason, escalation_ref}` on the original effect — always escalated and audited (WS-H6), never retried blindly, never reclassified as safe. Whether `abandoned` blocks run completion or only annotates it is OQ-WS-B2-07 (WS-G2/F1).
5. **Scope limits stated in the spec.** Compensation is best-effort semantic undo. It never applies to `irreversible` effects (no compensator can be registered for them), and reverting a trace/branch (WS-B4) never implies compensation of external effects unless the coordinator ran.

## Evidence (doc 3 §3.3 synthesis contract — all five mandatory)

- (a) Mechanism evidence: Sagas — compensation as semantic approximation, backward recovery in reverse order, reliable coordinator checkpoints — S-WS-B2-01 (Tier A); irreversible-action impossibility (why compensation cannot be universal) — S-WS-B2-09 (`provisional (mech)`, corroborated by S-WS-B2-01's own limits and by S-078's admission that trace revert does not undo external effects); git/branch discard as the workspace-local undo that compensation complements — S-041, S-052, S-107 (Tier B); saga-with-compensation applied to agents — S-WS-B2-06, S-WS-B2-07 (`provisional`; mechanism only).
- (b) Source-code precedent: workspace-local undo only — `codex/codex-rs/core/src/turn_diff_tracker.rs` (per-path baselines), `symphony/SPEC.md` §14.3 (workspace reuse/cleanup), OpenHands fork/branch (`local_conversation.py`). Compensator registration and a runtime saga coordinator over agent effects: `no-precedent / novel` in the audited harness corpus (the workflow-engine precedent is Temporal's saga pattern in user code, S-WS-B2-05; the agent-setting precedent is `provisional` research code, S-WS-B2-06/07).
- (c) Disconfirming evidence considered: compensation is only an approximation and compensators can fail or be irreversible themselves (S-WS-B2-01, S-WS-B2-09) — accepted, which is why `abandoned` exists and why a compensator may not be more dangerous than its original. No production harness needs it (S-107, S-108, S-110) — does not overturn: those harnesses confine effects to workspaces; the class exists for effects that leave the sandbox (PRs, tickets, reservations). LLM-driven recovery works in benchmarks (τ-bench-style) — the runtime coordinator does not forbid the model from *proposing* undo; it forbids undo from being *unrecorded* or *non-idempotent*.
- (d) Conditionality: value is task-class-conditional — near zero for sandboxed coding benchmarks (everything is `reversible`), high for operational/organizational tasks (doc 2 §10 "organizational semantics"). The schema is unconditional so that C1 can be added without changing C0 contracts. Assumes tool authors can write idempotent compensators; where they cannot, the effect is classified `irreversible` and takes the approval path.
- (e) Build-stage assignment: **C0 / Stage 1** — `CompensationPlan` schema, `compensation_plan_id` on `Effect`, `compensates` edge, authorisation precondition, `abandoned` event; **C1 / Stage 2** — saga coordinator (`compensate_run`), reverse-order execution, idempotent resume, AC-B2-04; **Stage 3** — crash-mid-saga fault injection in the evaluation suite.

**T-LCD statement (ADR-0007 decision 5).** Satisfies: T-LCD-12 (coordinator is an operation over ledger events, implementable out-of-process), T-LCD-06 (no hosting dependency; hosted participants simply have no compensators and their external effects are `irreversible`). Does not span models or targets, so T-LCD-01/-04/-11 do not apply except that a compensator template lowers like any `ToolCapability` (WS-A4 loss report).

## Consequences

- WS-A3 decides `CompensationPlan`'s entity status (OQ-WS-B2-03) and adds the `compensates` and `reverts` edges (or folds them into `produced-by` with a role) to the HIR edge set.
- WS-H1 must be able to evaluate a compensator template at `prepare` time; WS-H6 audits every `abandoned`.
- WS-B4's branch discard invokes `compensate_run(branch_id)` for applied `compensable` effects on the discarded branch; WS-B3 resumes an interrupted saga idempotently.
- The Stage 3 suite gains a compensable-effect probe tool with an external counter (AC-B2-01/-04).

## Reversibility

**High for the coordinator (C1)** — it can be descoped without touching C0 contracts because C0 only carries the schema and the `abandoned` state. **Medium for the schema** — removing `compensation_plan_id` after WS-H1/H7 key approval defaults on its presence would collapse `compensable` into `irreversible` and raise approval volume.
