# ADR-0327: CAP.3 `measurement.experiment.closed` audit-field partition ruling

- **Status:** Accepted
- **Date:** 2026-10-02
- **Ticket:** CAP.3
- **Requirement ids:** R-2.10.3
  (the experiment close row) ·
  R-2.8.6 (audit-grade class
  partitions) · DF-S3.12b-1.
- **Spec:** §6.3 §6 (the `closed`
  dossier) · §5g.6 (Rule C audit
  partitions) · ADR-0066/0067
  (the class-declaration surface).
- **Rung (ADR-0024 obligation):** **C1** — the experiment-engine's Lab-instrument rung; the class declaration is C0 ledger data.

## Context

`measurement.experiment.closed` carried `OPEN_AUDIT` — the audit-grade
class's wildcard member bound (`AUDIT_FIELD_MAX_BYTES = 512`). With every
matched dimension honestly measured, the E-4 close recheck writes
`utilization` (`{median, min, max, n}` per arm × matched dim) and
`budget_match.detail` (per comparand-group dim medians) — ~525–966 B of
measured detail at exemplar scale — and the append refuses
`AuditFieldsTooLarge` (DF-S3.12b-1, found at S3.12b, pinned by CAP.2's
composed spine). The specified path — an honestly-measured exemplar
closing `status: completed` — was unreachable.

## Decision

D1. **The class moves from the wildcard partition to an enumerated
dossier partition.** `EXPERIMENT_CLOSED_FIELDS` declares the close row's
own members: `status`, `coverage`, `outcome_counts`, `watermark_set`,
`summary_ref` at the scalar bound, and `under_utilised`,
`budget_match`, `na_cells`, `utilization` at
`AUDIT_FIELD_LIST_BYTES` — the declared bound for legitimately
record/list-shaped audit members (the same bound
`decider_provenance` carries). The row stays audit-grade (Rule C):
membership is closed, any undeclared member still refuses
`SchemaViolation`/`AuditFieldsTooLarge`, and an oversized member still
refuses at its declared bound.

D2. **The detail stays on the row, in place.** `utilization` /
`budget_match` are the close's own measured facts — an auditor reads
them where the verdict lives, not through a `content_refs` hop.

## Consequences

- An honestly-measured exemplar experiment closes:
  `cap2_experiment_exemplar_close_completes` and the s3_12b
  `assert_close_audit_cap` removal test both assert the landed
  `completed` row carries the recheck members the 512 B bound refused —
  removing the enumerated partition re-breaks them.
- The 512 B wildcard bound is unchanged for every other audit class;
  only this dossier class gets sized members.
- The comparison path is untouched — `budget_match` is computed
  pre-close from the runs' consumption; the ruling only changes what
  the close row may record.

## Alternatives considered

- **Raise `AUDIT_FIELD_MAX_BYTES` globally:** rejected — the cap is a
  deliberate smallness invariant over every audit row's members;
  widening it for one class's dossier weakens all of them.
- **`content_refs` offload for `utilization`/`budget_match`:** rejected —
  the recheck detail is the close's own fact set, not blob-scale
  payload; offloading makes every audit read two hops for data that is
  the row's content.
- **Shrink the members** (drop per-arm medians, summary-only):
  rejected — under-measuring the row to fit the cap is exactly the
  dishonesty the deferral pins; the members are the specified content.

## Revisit trigger

A legitimately record-shaped `closed` member exceeds
`AUDIT_FIELD_LIST_BYTES` (a wider matched-dimension set at exemplar
scale), or a second audit class needs member-level sizing — at that
point the per-member bound grammar, not another one-off partition, is
the ruling.
