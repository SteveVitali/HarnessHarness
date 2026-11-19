# CAP6.1 — whole-round gap analysis + seam hunt (manifest 321)

Independent, records-in capstone over Round-6 rows 253–320. **Verdicts were computed before reading any run ledger** (two-stage contract): Stage-1 artifacts committed at `084a41eb`; Stage-2 composed verification + run-ledger anchoring follows.

## Artifacts

- `docs/build/reports/round6/COVERAGE_MATRIX.csv` — 456 rows (one per register id), BM-VERDICT-01, required/achieved BM-STATUS-01 domains
- `docs/build/reports/round6/CAPSTONE_GAP_ANALYSIS.md` — method, ordering declaration, verdict roll-up, findings register, seam-hunt detail, GATE-G20 deviations with verbatim words, routing + proposed DF text
- `docs/build/reports/round6/COMPOSED_E2E_REPORT.md` — command ledger, pass/fail/skip counts, CI read
- `docs/build/reports/2026-11-19-cap6.1-gap-hunt.json` — machine-readable packet (`hh-cap6-gap-hunt/1`)
- `docs/build/tools/cap6_gap_hunt.py` — the records-in checker (never reads `docs/build/runs/*`)

## Verdict roll-up

**MET 409 · MET-DIFFERENTLY(ADR-0439) 1 · PARTIAL 46 · MISSING 0** — engineering-closed 410/456 (89.9%). PARTIAL = 41 routed→CAP6.2 + 4 scope-ended (GATE-G30 `re-scope{end-first-pass-early}`: REQ-XH-111, REQ-XH-115, REQ-XD-110, REQ-XD-119) + 1 unexecuted (REQ-XC-325, R-replay/SM-*). `withheld`/`inconclusive` outcomes stay typed; accepted deviations are represented as deviations citing GATE-G20's verbatim "accept all and proceed".

## Findings (15, all routed)

High: GAP-01 ledger GATE-DECISIONS misplacement (11 rows invisible to the guard → HOLD 'GATE-G28i pending'; G28i date cell lost in 0bffdb3c) · GAP-02 frozen-prereg arm-admission fixture drift (emit-identical FAIL). Medium: GAP-03/04/05/06/13. Low: GAP-07–12/14/15. Proposed rows DF-CAP6.1-1…5 in the gap report §6; **no coordinator-owned file was edited** — proposed LEDGER/BUILD_INDEX/DEFERRALS text is in `docs/build/runs/CAP6.1.md`.

## Requirement ids stamped

All 456 REQ-X register ids are stamped once in `COVERAGE_MATRIX.csv` (id column); the routed set names `CAP6.2` in `routing`.

## Verification

`cap6_gap_hunt.py` NC-1..NC-11/AC-1/AC-2 (NC-5b fail = GAP-01) · `check-supplement-guard.sh` FAIL/HOLD (recorded) · `check_round6_coverage.py` FAIL 34/35 (GAP-04) · `check-drift.sh` PASS · `check-site.sh` FAIL (GAP-13) · `check-position/live-evidence/bm-tools/living-docs/fp-select/fp-partition` PASS · `check-fp-prereg`/`check-fp-results`/`check-compute-receipts`/`gen-revisit-triggers` FAIL (GAP-02/09/14/08) · external build-memory validator 1 violation (GAP-15) · 114 r6 battery tests green, 0 failed, 6 ignored · full-workspace test skipped(budget).

**CI read (BM-CI-01):** `ci-boundary.sh --stack --no-wait`, `read_at: 2026-11-19T04:56:14Z` — `blockedOn: CI fail on #416`; stack #407–#412 pass, #414–#416 fail. Open PRs outside the stack read: #410, #413. This PR makes no production-state claim beyond that cited read.

## Boundary statement

Offline-only, $0, no secrets (`provided: no`), no infrastructure, no merges. Historical records untouched; all findings are append-only proposals.
