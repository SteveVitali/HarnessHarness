# REC.2 — spec reconciliation

**Stacked on:** `svitali/harnessharness-rec.1` (PR #100) · **Do not merge — operator-controlled**

## Summary

`reconcile-build mode=spec`: what the 92-ticket build shipped is reconciled
against `spec/CANONICAL_SPEC.md`. Every landed chain row's deliverables are
tagged in-spec / spec-implied / ticket-added with dispositions
(`docs/build/TICKET_VS_SPEC.md` — **in-spec 389 · spec-implied 114 ·
ticket-added 17** tagged groups; nothing contradicts the spec). The fold-back
amendments are proposed, unticked (`docs/build/SPEC_RECONCILIATION_PLAN.md` —
**3 proposed / 0 applied**; amendments apply only where the operator ticks).
The four BL-30 ruling rows are ruled in **ADR-0331**: `debt.hypothesis`
string-canonical (DF-S1.24-3 → DONE), parked-detach declared `detached` with
no `ExitClass` widening (plan A-1), signer custody external to
WS-H3/H6/L4 (OQ-170), OQ-388 measurement folded for WS-B1/K2 ratification
(plan A-2). The docs/adr set (0217–0330) vs Appendix B (0001–0216) delta is
disjoint-by-construction and dispositioned (plan A-3 / BM-ADR-04).

Requirement ids stamped: `R-2.8.6` · `R-2.2.1` · `R-2.9.4` · `R-2.9.6` ·
`R-2.11.1` (the BL-30 ruling family).

## Deliverables

- `docs/build/TICKET_VS_SPEC.md` — 98 landed rows, each deliverable tagged +
  dispositioned (`ID` / `AX` / `FB→plan`).
- `docs/build/SPEC_RECONCILIATION_PLAN.md` — amendments A-1…A-3 proposed
  (target file, anchor, before/after, tick `[ ]`); A-4…A-8 dispositioned
  (external/signed/already-carried); fold-back id accounting (no new ids);
  ADR-appendix parity section.
- `docs/adr/ADR-0331` + regenerated `docs/adr/README.md`.
- `docs/tickets/DEFERRALS.md` — DF-S1.24-3 → DONE; progress notes on
  DF-S1.26-2 / DF-S2.5-1 / DF-S4.13-1 (stay OPEN).
- `docs/build/BACKLOG.csv`/`BACKLOG.md` — ADR-0331 placed at BL-30; sweep
  tally 115.
- `docs/build/runs/REC.2.md`, this file, BUILD_INDEX row 100, LEDGER.

## Verification

- `check-backlog.sh` → exit 0 (146 sources, placed once).
- `check-build-memory.sh` → 0 violations / 7 pre-existing warnings.
- `grep -n '^||' docs/tickets/DEFERRALS.md` → empty.
- `cargo test --workspace` not run — reconciliation ticket, no code change.
- All writes via shell/python; `git diff`/`git show --stat` verified.

## For the operator

To apply amendments: tick `[x]` on plan rows A-1/A-2/A-3, then re-run
`reconcile-build mode=spec apply_amendments=true` — it edits
`spec/sections/*` + the inlined copy in `spec/CANONICAL_SPEC.md` and appends
the manifest `## Spec amendments applied` lines. A-4/A-5/A-6 await named
workstreams (WS-H3/H6/L4 · WS-I5/J4 · WS-K1/K4/F2); A-2's fold also awaits
WS-B1/K2's ratification act.
