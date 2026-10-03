# ADR-0328: CAP.3 hosted `security.permission.*` lift — partition shaping, not verbatim pass-through

- **Status:** Accepted
- **Date:** 2026-10-02
- **Ticket:** CAP.3
- **Requirement ids:** R-2.10.6
  (the Hosting ABI lift) ·
  R-2.8.6 (audit-grade class
  partitions) · DF-CAP.2-1.
- **Spec:** §5g.7 (the permission
  classes) · §6.6 §9 (the lift) ·
  ADR-0164/0165/0166 (the ABI
  rulings) · ADR-0066/0067 (Rule C).
- **Rung (ADR-0024 obligation):** **C2** — the hosted-lift rung; the partitions it feeds are C0 ledger declarations.

## Context

`hh_hosting::proj::lift` passed hosted `permission.requested` /
`permission.decided` payload members verbatim onto
`security.permission.{pending,decided}` — including `params` and
`approval_wait_ms`, which the kernel's Rule-C audit partitions do not
declare. `lab.hosting.attach`'s batch append therefore refused
`SchemaViolation` on the first lifted permission row: a hosted session
containing a Π ask could never land its permission lifecycle on the
subject run (DF-CAP.2-1, found by the CAP.2 composed spine).

## Decision

D1. **The lift *shapes*; it never passes through.** Hosted upcall rows
map onto the native partitions' declared members:
`permission.requested` → `security.permission.pending{permission_id,
request}` where the hosted `params` upcall record lands *in full* as
the partition's `request` member (record bound — nothing the
participant did not report is minted, and no member is silently
dropped); `permission.decided` →
`security.permission.decided{permission_id, decision, decider,
wait_ms}` where hosted `approval_wait_ms` names the native `wait_ms`.

D2. **The partitions admit the observation mark, not hosted spellings.**
`security.permission.{pending,decided}` declare a `provenance` member —
the attach kernel mint stamps `participant_reported` on every lifted
audit-grade row (CC2: the participant's claim is data, never a kernel
fact), the same accommodation `OPEN_AUDIT`'s `*` gives every other
lifted class. `AdapterA` stamps the `permission_id` correlator on
`decided` so the lifted pair joins pending ↔ decided.

## Consequences

- A hosted session's permission lifecycle lands durably on the subject
  run through `lab.hosting.attach` —
  `cap2_hosted_attach_permission_rows_land` runs green un-ignored
  (DF-CAP.2-1's done check verbatim).
- The partition member-check keeps its teeth:
  `cap2_hosted_attach_permission_rows_refuse` still refuses
  `SchemaViolation` — now by injecting an undeclared member into a
  lifted row, the guard the defect exercised.
- The native `security.permission.*` spellings stay single-shaped
  (CC7) — hosted aliases never entered the kernel partitions.

## Alternatives considered

- **Amend the partitions to carry hosted members** (`params`,
  `approval_wait_ms`): rejected — it pollutes the §5g.7 kernel
  partitions with hosted spellings; the partition is the contract the
  lift must meet, not the other direction.
- **Drop the undeclared members at lift:** rejected — silent data
  loss; `params` carries the ask's substance. Shaping preserves all
  of it inside `request`.
- **A separate hosted-permission class pair:** rejected — the lifted
  row IS the kernel's permission lifecycle; a second class would fork
  the audit story for asks the kernel already governs (CC1).

## Revisit trigger

A new hosted upcall class carries members no honest shaping maps onto
the native partition — at that point a lift-table entry is ruled per
class, never a verbatim pass-through.
