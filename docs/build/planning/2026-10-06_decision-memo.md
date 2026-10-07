# Decision memo — seed for the next `decompose-spec mode=extend` round

> Skeleton written by REC.3 (`reconcile-build mode=integration`),
> 2026-10-06. This is the bridge from closeout back into planning:
> what shipped, what the backlog carries forward, the open operator
> decisions, and the candidate scope for round 2. The sequencer owns
> what happens next; this memo only hands it the facts.

## What shipped (round 1)

- **99 landed chain rows** — S0.1 … S6.4 tickets, 3 capstone tickets,
  3 reconcile tickets — one PR per ticket (#2–#102), four gates
  (G1 / G2 / G3 / GATE-ACCEPT) all PASSED.
- Suite at capstone: **280 result blocks / 3028 tests / 0 failures /
  2 ignored** (`82ef6b0`); CC1–CC10 capstone re-check clean;
  `check-build-memory` 0 violations.
- **Integration state:** 86 chain PRs already merged into `main` by the
  operator; the remaining 15 (#87–#101) + REC.3's own PR merge clean
  bottom-up per `docs/build/INTEGRATION_PLAN.md` (dry-run evidence:
  `docs/build/reports/merge-dryrun-2026-10-06.json`, 15/15 clean).
- Spec reconciliation (REC.2): 389 in-spec / 114 spec-implied /
  17 ticket-added deliverable groups; **3 amendments proposed, 0
  applied** — nothing contradicts the spec.
- Backlog (REC.1): 43 rows, every owed source placed exactly once
  (`check-backlog.sh` green, 146 sources).

## What the backlog carries forward

| row | theme | lands when |
|---|---|---|
| BL-31 | Live provider transports + TLS-terminating transport (DF-S1.18-1, DF-S2.4-2; ADR-0253/0284/0313) | real provider endpoints/credentials exist; non-loopback egress authorized |
| BL-32 | Remote fetch transports + foreign import/export vocabularies (DF-S4.2-1/-2; ADR-0292–0294, ADR-0308) | WS-L6 trust-field ruling + owning tickets |
| BL-11…BL-28 family | Producer-side emitter wiring (the PARTIAL matrix is emitter debt, not missing types) | round-2 implementation tickets |
| BL-33/BL-34/BL-38 | program-resume package (WS-L6 window: ADR-0210 deferrals, ADR-0211 set, ADR-0215 candidates) | operator opens the WS-L6 window |
| BL-35…BL-40 | register revalidation packages (ADR-0212…0216 stage-blocking OQs; spec-debt register sweep) | round-2 revalidation tickets |
| BL-41/BL-43 | standing (edit-tool stale-view rule; fired-ADR sweep) | accepted |

## Open operator decisions

1. **Land the chain** — the copy-pasteable procedure in
   `docs/build/INTEGRATION_PLAN.md` §6 (bottom-up, retarget to `main`,
   merge commits). Not blocking round-2 *planning*.
2. **Spec amendments** — tick A-1/A-2/A-3 in
   `docs/build/SPEC_RECONCILIATION_PLAN.md`, then re-run
   `reconcile-build mode=spec apply_amendments=true`.
3. **A-2 dependency** — the OQ-388 `filtered_read` bound needs the
   WS-B1/K2 ratification act before its fold applies.
4. **Human rows** — HUMAN-H1 (bind surface ecosystem) and HUMAN-H2
   (real tracker + signed webhook; credentials provided: no) stay
   pending; non-blocking for closeout.
5. **`projectStatus → DONE`** — flips only after DOC.1–DOC.2 land and
   the chain is fully on `main` (manifest rule).
6. **Round-2 scope selection** — pick the backlog packages the next
   round ingests (below).

## Candidate scope for round N+1

Ranked candidate packages for `decompose-spec mode=extend` to expand:

- **Emitter-wiring wave (BL-11…BL-28).** Largest debt class; every row
  is "wire the declared member/op's producer" — mechanical,
  parallelizable, test-per-row.
- **Transport/vocabulary package (BL-31 + BL-32).** Real provider
  transports, TLS termination, remote fetch, remaining foreign codecs —
  needs the external-dependency preconditions above.
- **Register revalidation (BL-35…BL-40).** The spec's own deferred-ADR
  debt: 103 open questions + the spec-debt sweep — governance work, no
  new runtime surface.
- **WS-L6 program-resume window (BL-33/34/38)** when the operator
  reopens that workstream.

## Inputs the next round needs

- `docs/build/BACKLOG.csv` (authoritative owed-work register),
  `docs/build/TICKET_VS_SPEC.md` (landed-vs-spec map),
  `docs/build/SPEC_RECONCILIATION_PLAN.md` (pending folds),
  `docs/build/INTEGRATION_PLAN.md` (landing state),
  `docs/tickets/DEFERRALS.md` (row-level detail).
