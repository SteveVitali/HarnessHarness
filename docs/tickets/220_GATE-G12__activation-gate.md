# GATE-G12 — Authorize activation then fixed pilot

- **Sequence:** 220 of 246 · **Phase:** Round 5 · **Kind:** gate
- **base_branch:** current checkout
- **Depends on:** R5.45, R5.42, HUMAN-H8, R5.40
- **Readout:** `docs/build/readouts/GATE-G12.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign or infer approval.

## Criterion (verbatim from the spec)

> Unset funds=$0; finite exposure missing→zero dispatch; no automatic top-up/cloud fallback.

Source: [s9](../evidence-and-adoption-spec.md#s9).

## Pre-registered thresholds

Record separate one-task activation quote and, only after its result, explicit fixed pilot authorization ≤60 first subjects under §9.2 per-subject and tranche ceilings. Freeze tasks/splits/policies/caps/oracle/detectors/analysis/stops before data; no implied permission for next stage. One gate can have separate numbered readings, neither pre-signed.

## Disposition

Record exact operator words, UTC, channel, scoped items, expiry/void conditions and explicit confirmation of the agent-draft hash. Append readout/signature and matching GATE DECISIONS; old acceptance never substitutes. A pending or failed gate stops dependent dispatch. A scoped decision must name carried unmet predicates and compensating controls; no favorable-data change to thresholds.

## DEFERRALS rule

No OPEN scoped technical deferral is silently passed. Resolve it or obtain an explicit protocol-compliant disposition with owner/trigger and withheld claims; Q-7 and this decomposition are not that disposition.

## Operating clauses

Follow [Round-5 operating rules](00_MANIFEST.md#operating-rules) and [I-01–I-08/O-01–O-08](../build/planning/2026-10-30_evidence-invariants.md). No code branch is created by a gate.
