# CAP5.1 — Independent whole-scope gap hunt

**Summary.** First round-5 capstone: independent records-in hunt over the whole landed round — every packet (executed preflight/useful/ops-live/activation/pilot; withheld/unselected parity/confirmation/plugin/interchange/outside/formative/publication/value; study + sim records), every gate decision (G18/G11/G12 readings/G13-N0/G14-quote-only/G15/G16 out-of-scope + H8–H11), every deferral and every REQ/coverage anchor. No findings manufactured — 4 real gaps found, all repaired append-only.

**What changed**
- `docs/build/tools/cap5_gap_hunt.py` — records-in checker, twelve checks NC-1…NC-12 (packet inventory, disposition re-fold, gate↔record verbatim, deferral-reference integrity, register ownership/anchor/contributor integrity, status honesty, BUILD_INDEX + PHASE LOG consistency, over-claim scan, coverage⇄contributor binding, owed-work homes, readout completeness).
- `docs/build/reports/round5/COVERAGE_MATRIX.csv` — 180 rows (167 adopted + 12 merged + 1 superseded), BM matrix columns + domain/owed/scope columns; verdicts BM-VERDICT-01.
- `docs/build/reports/round5/CAPSTONE_GAP_ANALYSIS.md` + `docs/build/reports/2026-11-04-cap5.1-gap-hunt.json` (hh-capstone-gap-hunt/1).
- Record repairs (append-only): `DEFERRALS.md` +DF-H9-1 (P) +DF-CAP5.1-1 (V); LEDGER GATE DECISIONS +G12 reading-2 row +G15 timestamp-correction row.
- Contract 238 dated note (NC-1…NC-12 bound), manifest 238 note, run ledger, PHASE LOG fleshing entry.

**Findings (real, all repaired)**
1. DF-H9-1 cited `open` by 6 packets + G15/G16 + readouts + manifest — row never existed in DEFERRALS.md → minted verbatim.
2. GATE-G12 Reading-2 PASSED never got a GATE DECISIONS row (table read PENDING; pilot executed on it) → row appended verbatim.
3. GATE-G15 decided_at drift 14:32:56Z vs verbatim 14:32:29Z → dated correction row.
4. Failed S1 residual (REQ-A-003/A-104 + contributor legs) orphaned — no DF row, no contributor → DF-CAP5.1-1 minted, routed GATE-G17.

**Matrix rollup:** MET 78 · MET-ENGINEERED 29 · PARTIAL 73 · MISSING 0 — engineering closed 107, requirement satisfied 78.

**Verification.** `python3 docs/build/tools/cap5_gap_hunt.py --matrix` → 12/12 checks pass, 0 open findings · `check-bm-tools.sh` OK · `check-living-docs.sh` OK (438 files) · `check-build-memory.sh .` 0 violations (warnings pre-existing) · guard `PASS route DISPATCH — CAP5.1` · fmt n/a (no Rust; docs+tools only) · changed source/test lines 701 ≤750.

**Requirement IDs:** contributing slice only — none to stamp (contract records none).

| AC | verdict | evidence |
|---|---|---|
| AC-1 whole-scope check | met | NC-1..NC-12 pass; matrix 180 rows; gap-hunt JSON |
| AC-2 no inflation | met | fold/status/over-claim scans clean; no fabricated checkmarks |
| AC-3 achieved vs owed | met | per-row required/achieved domain + owed_legs + routing homes |
| AC-4 scoped verification | met | checker + docs checks; fails-without by construction of check predicates |
| AC-5 phase gate | met | deferrals minted; ledger/index/phase-log consistent at close |
