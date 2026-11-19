# CAP6.1 — run record

- **Ticket:** `docs/tickets/321_CAP6.1__cap1.md` (manifest row 321, capstone)
- **Branch:** `codex/harnessharness-cap6.1` — base `31fc815b` (`codex/harnessharness-r6.50` tip, HUMAN-H18)
- **Harness:** Devin (Claude Opus 5.5 xhigh), isolated worktree `/tmp/hh-cap61-wt`; offline, $0, no secrets, no paid execution, no merges.
- **Depends on:** R6.50 — satisfied: HUMAN-H18 readout signed PASSED; publication records + flips committed and pin-verified.
- **Guard read at dispatch:** `check-supplement-guard.sh` → `FAIL`, `route: HOLD — gate GATE-G28i pending — RETURN PASS`. The HOLD is the GAP-01 symptom (decision rows present but outside the parsed `## GATE DECISIONS` region) — CAP6.1 is a records-only capstone whose inputs do not depend on the held gate; proceeded per contract while recording the HOLD verbatim.

## What was done (ordered)

1. Read DEFERRALS first (`deferrals-open.sh`: DF-R6.45l-1 genuinely OPEN; DF-R6.50-1 lead-OPEN + dated DONE — scanner seam), manifest rows 300–321, LEDGER, CAP6.1 contract, spec §0.2/Appendix A, coverage register, decomposition, invariants.
2. **Stage 1 (before any `docs/build/runs/*` prose):** wrote `docs/build/tools/cap6_gap_hunt.py` (records-in, read-only); emitted `docs/build/reports/round6/COVERAGE_MATRIX.csv` (456 rows, BM-VERDICT-01) + `docs/build/reports/2026-11-19-cap6.1-gap-hunt.json` + `docs/build/reports/round6/CAPSTONE_GAP_ANALYSIS.md`. Committed `084a41eb`.
3. **Seam hunt:** shared-decision table (38 rows), withhold/n-a/unused token census, every `.sha256` sidecar (35) + publication embedded pins, gate dispositions vs window records, deferral verify clauses, `n/a{scope_end}` honesty, fabricated-label sweep, known checker drift.
4. **Stage 2:** read the required run ledgers as landed-evidence anchors; ran the composed offline verification — `docs/build/reports/round6/COMPOSED_E2E_REPORT.md`.

## Verdicts (independent, recorded pre-ledger)

MET 409 · MET-DIFFERENTLY(ADR-0439) 1 · PARTIAL 46 · MISSING 0 — engineering-closed 410/456. PARTIAL = 41 routed→CAP6.2 + 4 scope-end (XH-111, XH-115, XD-110, XD-119 under GATE-G30) + 1 unexecuted (XC-325). `inconclusive`/`withheld` study outcomes remain typed; no accepted deviation is a plain pass.

## Findings (full register: CAPSTONE_GAP_ANALYSIS.md §2)

GAP-01 ledger GATE-DECISIONS-region misplacement (11 decision rows invisible to the guard; G28i date cell lost — append-only violation) · GAP-02 frozen-prereg arm-admission fixture drift (emit-identical FAIL) · GAP-03 XC-325 no executor · GAP-04 register↔plan primary_kind drift (10 ids) · GAP-05 manifest ordering + annotated Depends cells + stale Kind:skeleton + criterion heading variant · GAP-06 DF-R6.50-1 lead-token/dated-disposition seam · GAP-07 phantom DF-R6.45b-1/-2 · GAP-08 revisit-triggers ADR-0450/0451 unplaced · GAP-09 compute-receipts crash on [] record · GAP-10 fp_e_emit.rs:189 panic clippy · GAP-11 H17 live-executed vocabulary note · GAP-12 stock-checker GATE-G30.md false positive · GAP-13 check-site REFUSED on flip filenames (site unrendered) · GAP-14 fp-seal-open byte-identical FAIL · GAP-15 docs/adr/README.md stale.

## Verification (verbatim results — COMPOSED_E2E_REPORT.md)

pass 9 · fail 9 · informational 2; all non-green routed to CAP6.2 via proposed DF-CAP6.1-1…5 text in the gap report (coordinator-owned files untouched). Batteries: 114 r6 tests green (41+73), 0 failed, 6 ignored; full-workspace `cargo test` and `check-removability.sh` **skipped(budget/heavy-rebuild)** — honest bounds, owed legs folded into DF-CAP6.1-5 — honest bound, owed leg to CAP6.2.

**CI read (BM-CI-01, `ci-boundary.sh --stack --no-wait`, `read_at: 2026-11-19T04:56:14Z`):** stack verdict `blockedOn: CI fail on #416` — #416/#415/#414 fail build·test·fmt·drift (ubuntu+macos), #407–#412 pass, #410/#413 not in the stack read. No production-state claim beyond this cited read.

## Proposed coordinator-file text (not edited by this worker)

- `DEFERRALS.md`: the five proposed rows DF-CAP6.1-1…DF-CAP6.1-5, verbatim in CAPSTONE_GAP_ANALYSIS.md §6.
- `BUILD_INDEX.md`: append a CAP6.1 row — seq 321, kind capstone, evidence `engineered` (records-in analysis; live-verification `n/a`), artifacts `docs/build/reports/round6/{COVERAGE_MATRIX.csv,CAPSTONE_GAP_ANALYSIS.md,COMPOSED_E2E_REPORT.md}`, run `docs/build/runs/CAP6.1.md`, pr `docs/build/pr/CAP6.1.md`.
- `LEDGER.md`: `lastCompleted → CAP6.1` + `nextTicket → CAP6.2` upon merge; PHASE LOG entry summarizing this run; the GAP-01 corrective decision rows per DF-CAP6.1-1.
