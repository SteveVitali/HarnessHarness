<!--
  Round-5 run-time insert — S4-completion extension (rows 247–251; manifest
  `## Plan extensions` entry 2026-11-06). A milestone gate authored under the operator's
  verbatim authorizations: "insert these into the build plan as new ticket(s), then
  continue driving orchestrate-build over them to the end" + "you can provision
  everything. just make sure we don't forget to deprovision at the end. proceed"
  (operator chat, received 2026-11-06).
-->
# GATE-G19 — Rule the efficacy-inference admission over the completed cohort

- **Sequence:** 250 of 251 · **Phase:** Round 5 · **Kind:** gate
- **base_branch:** current checkout
- **Depends on:** R5.58
- **Readout:** `docs/build/readouts/GATE-G19.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign or infer approval.

## Criterion (verbatim from the spec)

> Fixed sample/final analysis; integrity/treatment/cap/isolation/leakage/drift/ceiling failure stops and retains partial results/costs. No favorable peeking/top-ups or retrospective smaller manifest. Feasibility/precision failure leaves objective unmet with owner/trigger, not waived or null.

Source: [s9](../evidence-and-adoption-spec.md#s9) (§9.2) and [s4](../evidence-and-adoption-spec.md#s4) (§4.3 — missing/unequal cells keep descriptive values/bounds plus `InsufficientReplicates` or unavailable inference; §4.1 — post-freeze changes retain original rows and dated changes).

## Pre-registered thresholds

The ruling admits exactly one efficacy-inference basis for R5.59, on the **real post-completion state** (the R5.58 `hh-s4-completion/1` record: executed/inconclusive verbatim per slot — the ~290/300 executed ceiling given `fix-git`'s permanent `upstream_deleted`):

- **complete-cohort** — the registered finite-manifest efficacy contrast (REQ-D-107/109, §4.3 machinery admitted at GATE-G9) computes only when every one of the 300 cohort slots carries a real receipt; any permanent inconclusive residue leaves the efficacy inference `unavailable{incomplete_cohort}` and S4 honestly partial — no efficacy verdict is produced and nothing is inferred from the executed subset.
- **bounded-inference** — the registered contrast computes over the executed base with missingness declared verbatim per slot (which slots, which reason codes, which tasks/arms/rounds they sit in); the residual is declared, never dropped or silently shrunk — never a retrospective smaller manifest; unavailable members stay inside the frozen multiplicity family per §4.3/§3.2 and the verdict's label names its basis.

No efficacy inference is produced before the ruling; no favorable-data change to thresholds; the presented state is the executed record verbatim (counts, denominators, residue, costs), not a projection.

## Disposition

Record exact operator words, UTC, channel, the admitted basis verbatim (complete-cohort or bounded-inference with its declared missingness scope), expiry/void conditions and explicit confirmation of the agent-draft hash. Append readout/signature and matching GATE DECISIONS; old acceptance never substitutes. A pending or failed gate stops dependent dispatch. A scoped decision names carried unmet predicates and compensating controls; a bounded-inference admission names the exact residue it covers — nothing broader is admitted.

## DEFERRALS rule

No OPEN scoped technical deferral is silently passed. Resolve it or obtain an explicit protocol-compliant disposition with owner/trigger and withheld claims; Q-7 and this decomposition are not that disposition.

## Operating clauses

Follow [Round-5 operating rules](00_MANIFEST.md#operating-rules) and [I-01–I-08/O-01–O-08](../build/planning/2026-10-30_evidence-invariants.md). No code branch is created by a gate.
