# CAP5.3 — Close layers and prepare fresh acceptance

**Spec:** `docs/tickets/240_CAP5.3__cap3.md` (seq 240 of 246, role CAP.3)
**Base:** `codex/harnessharness-evidence-cap5.2` @ `2909edce`

## Summary

Round-5 closure recompute, records-in and offline-only (USD $0, no
task/provider/container/participant execution): a new checker
`docs/build/tools/cap5_closure.py` (`hh-cap5-closure/1`, 10 checks
CL-1…CL-10) folds the five AC-9-3 layer verdicts from committed packet
dispositions — never hand-asserted — recomputes engineering-closed vs
requirement-satisfied totals + the highest domain ceiling from CAP5.1's
matrix, verifies unique owed-work homes and withheld-never-promoted
posture, reconciles CAP5.1's four findings and adjudicates the LEDGER
capstone items, emits the three-part GATE-G17 signature list and the
fresh-acceptance prep block. Emitted packet
`docs/build/reports/2026-11-04-cap5.3-closure.json` (disposition
`reported`, `packet_id 9d756c68…`) + report
`docs/build/reports/round5/CAPSTONE_CLOSURE.md`. Contract 240 fleshed
per the capstone ritual (dated note binding CL-1…CL-10 + manifest
row-240 note + PHASE LOG entry).

## Layer verdicts (real, packet-folded)

engineering **executed** · research **inconclusive** (reported honestly —
`unmet{efficacy_unavailable;parity_withheld}` verbatim; S1 failed,
parity/confirm withheld) · portability **withheld** · public **withheld**
· adoption **withheld**. Totals: eng-closed **107** / req-satisfied
**78** / ceiling `engineered` (MET 78 / MET-ENGINEERED 29 / PARTIAL 73).
**The build is not DONE** — next is GATE-G17 fresh acceptance.

## Verification

- `python3 docs/build/tools/cap5_closure.py` → **10/10 pass, 0 findings**
- guard `PASS route: DISPATCH — CAP5.3` pre+post (82 rows/180 cand/0 viol)
- `cargo fmt --all --check` clean · living-docs 456 OK · bm-tools OK
- `check-ci-gating` OK (14/14 allowlisted) · check-build-memory `.` 0 viol
- changed source/test lines **572 ≤600** (`cap5_closure.py` only)

## Requirement IDs stamped

REQ-C-005, REQ-H-009 (primary — achieved D/E this ticket; R/P legs stay
owed to GATE-G17/DF-H9-1; no deferred full-target predicate becomes MET).

## AC verdicts

| AC | verdict | evidence |
|---|---|---|
| AC-1 closure report + reconcile + prep | met | packet + report; CL-1..CL-10 |
| AC-2 withheld never complete; no old signatures | met | CL-6/CL-7 negative scans |
| AC-3 predicates→checks→evidence | met | CL-3 register resolution + report target table |
| AC-4 scoped verification | met | 10/10 + fmt/docs/bm/guard green; 572≤600 |
| AC-5 phase gate | met | DF rows carried OPEN; projectStatus IN_PROGRESS; records agree |

Deferrals: none opened/closed — carried DF-R5.1-1, DF-H9-1, DF-CAP5.1-1.
