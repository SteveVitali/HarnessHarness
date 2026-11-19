# Round-6 backlog — reconciled view (CAP6.2 stage 2 of 3)

2026-11-19 · generated per `reconcile-build mode=backlog` over the committed build memory.
Canonical register: `docs/build/BACKLOG.csv` (row set unchanged by this ticket except the
dated BL-47 cell appends ADR-0450/ADR-0451 and a stray trailing `;` removed — inherited
drift, recorded). `check-backlog.sh` → **complete + non-duplicating + verdict-consistent**
(235 expected sources; two sums 66/66 = engineering closed = requirement satisfied for the
register's own matrix scope; MET-ENGINEERED never counted as MET).

This round-scoped view (`reports/round6/BACKLOG.csv`) folds the CAP6.1 coverage matrix:
**46 PARTIAL, 0 MISSING** rows —

- **34 ids routed** to the later pass → `LATER_PASS_ROUTING.md` (stage 3; routed, never satisfied).
- **5 ids discharged by this ticket's artifacts:** REQ-XH-110 (recompute), REQ-XH-021
  (spend kind/phase), REQ-XH-029 + REQ-XH-106 (live reads), REQ-XH-015 (history check).
- **REQ-XC-325** — `unexecuted{no_executor}` + waiver request (GATE-G32 list item 3).
- **REQ-XD-110 / REQ-XD-119** — typed `protected_pair_truncated{k<5,R<5}` / `sampled_n=0`
  honest outcomes under `re-scope{end-first-pass-early}` — stand verbatim, carried to the
  later pass's re-execution decision.
- **REQ-XH-111 / REQ-XH-115** — W4 legs `unused{scope_end}` verbatim (GATE-G30 arm).
- **REQ-XD-124 / REQ-XE-127** — aliases; verdicts inherited at REQ-XE-013 / REQ-XD-205.

Open deferral rows at closure: none owed to Round 6 beyond the routed register
(DF-R6.23-1 DONE; DF-R6.45l-1 DONE via R6.51/PR #413; DF-R6.50-1 DONE via HUMAN-H17;
DF-CAP6.1-1…5 dispositioned in CAPSTONE_CLOSURE.md §"CAP6.1 routed gaps").
