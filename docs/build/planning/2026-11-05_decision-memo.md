# Decision memo — seed for the next `decompose-spec mode=extend` round

> Skeleton written by REC5.3 (`reconcile-build mode=integration`),
> 2026-11-05. The bridge from closeout back into planning: what
> shipped, what the backlog carries forward, the open operator
> decisions, and the candidate scope for round 6. The sequencer owns
> what happens next; this memo only hands it the facts.

## What shipped (round 5)

- **64 landed chain rows** — R5.1…R5.55 tickets (+ split legs), three
  capstones, two reconciles — one PR per ticket (#209–#275 landed or
  open; REC5.3 = #276). Gates G9/G10/G13/G14/G15/G16 passed en route;
  **GATE-G17 PASSED 2026-11-05T00:13:07Z** — fresh round-5 acceptance
  on the honest record ("passed, continue").
- Layer fold (CAP5.3, `reports/2026-11-04-cap5.3-closure.json`
  `9d756c68…`): engineering **executed** · research **inconclusive**
  (`unmet{efficacy_unavailable;parity_withheld}`) · portability /
  public / adoption **withheld** · `build_status: not_done`;
  engineering-closed 107 / requirement-satisfied 78 (of 180 candidates).
- Integration state at the 2026-11-05 read: #209–#259 merged; open
  #260–#275 all MERGEABLE + CI-pass at heads, 16/16 clean onto
  `origin/main` (`reports/round5/INTEGRATION_PLAN.md` +
  `merge-dryrun-2026-11-05.json`).
- Reconciles (REC5.1/REC5.2): 73 PARTIAL matrix ids + 3 OPEN deferrals
  homed exactly once in BL-50…BL-53; 63 landed rows tagged vs the spec
  (1 proposed unticked amendment R5A-1; 0 applied); REQ-E-210 resolved
  E210-1…7 at D/E.

## What the backlog carries forward

| row | theme | lands when |
|---|---|---|
| BL-50 | 40 scope-disposition ids — carried R/P predicates | next-round scope dispositions |
| BL-51 | participant predicates (DF-H9-1 verbatim; HUMAN-H9) | actual named participants/reviewer + consent/scope/access/resources |
| BL-52 | failed-S1 residual (DF-CAP5.1-1; + R5.43 legs) | a named bounded S1 re-execution admission, or verbatim unmet-by-decision |
| BL-53 | contributor-evidence cells (DF-R5.1-1) | boundary guard-invocation records cited in the cells |
| BL-01 | foreign toolchains (environment-pending) | toolchains available |
| BL-47/48, BL-49, BL-41/43 | standing accepted/index rows | standing |

## Open operator decisions

1. **Land the chain** — the copy-pasteable bottom-up procedure in
   `reports/round5/INTEGRATION_PLAN.md` §6 (#260 → #276, retarget to
   `main`, merge commits). Then DOC5.1/DOC5.2 land the docs tail before
   `projectStatus` can flip.
2. **Spec amendment R5A-1** — tick (or reject) the one proposed `[ ]`
   row in `reports/round5/SPEC_RECONCILIATION_PLAN.md` (frozen appendix
   ADR-set range 0330→0420), then `reconcile-build mode=spec
   apply_amendments=true`.
3. **HUMAN rows** — HUMAN-H8 (benchmarkSet binding, `benchmarkSet:
   (unset)` in LEDGER) and HUMAN-H9 (participant supply; DF-H9-1/BL-51)
   stay pending; withheld claims stay withheld.
4. **Round-6 scope disposition** — the BL-50 carried predicates each
   need an explicit next-round disposition; Q-7 stays open until human
   ratification (AC-9-3).

## Candidate scope (round 6) — inputs only, nothing allocated

- Discharge path for BL-50…BL-53 under their named triggers.
- The S1 useful-task residual: admit a bounded re-execution on the
  repaired machinery, or record unmet-by-decision verbatim.
- Participant/outreach stages remain BLOCKED pending the DF-H9-1
  supply trigger — not candidate work.
- Docs tail of *this* round (DOC5.1/DOC5.2) runs first — it is inside
  round 5, not round-6 scope.
